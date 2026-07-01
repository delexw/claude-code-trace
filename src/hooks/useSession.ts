import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "../lib/invoke";
import type {
  DisplayMessage,
  TeamSnapshot,
  SessionMeta,
  SessionTotals,
  LoadResult,
  GitInfo,
  DebugEntry,
} from "../types";
import { useTauriEvent } from "./useTauriEvent";

/** Messages are fetched and evicted a page at a time. Only pages near the
 * viewport are kept in memory, so the JS heap stays flat regardless of how many
 * messages the session has. */
const PAGE_SIZE = 100;
/** Pages within this many pages of the viewport are kept; the rest are evicted. */
const KEEP_MARGIN_PAGES = 1;

/** Session-level state (everything except the windowed message bodies). */
interface SessionState {
  /** Total message count in the session. */
  count: number;
  /** Role of every message (length === count) — the lightweight index. */
  roles: string[];
  /** Latest Claude context-window fill (tokens); 0 if none. */
  contextTokens: number;
  teams: TeamSnapshot[];
  ongoing: boolean;
  meta: SessionMeta;
  sessionTotals: SessionTotals;
  sessionPath: string;
  gitInfo: GitInfo | null;
  debugEntries: DebugEntry[];
  loading: boolean;
}

/** Lightweight live signal emitted by the watcher (no message bodies). */
interface SessionUpdateSignal {
  count: number;
  roles: string[];
  context_tokens: number;
  teams: TeamSnapshot[];
  ongoing: boolean;
  permission_mode: string;
  session_totals: SessionTotals;
}

const emptyMeta: SessionMeta = {
  cwd: "",
  git_branch: "",
  permission_mode: "",
};

const emptyTotals: SessionTotals = {
  total_tokens: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  cost_usd: 0,
  model: "",
};

const emptyState: SessionState = {
  count: 0,
  roles: [],
  contextTokens: 0,
  teams: [],
  ongoing: false,
  meta: emptyMeta,
  sessionTotals: emptyTotals,
  sessionPath: "",
  gitInfo: null,
  debugEntries: [],
  loading: false,
};

const pageOf = (index: number) => Math.floor(index / PAGE_SIZE);

export function useSession() {
  const [state, setState] = useState<SessionState>(emptyState);
  // Sparse store of loaded message bodies, keyed by absolute index. Replaced
  // (new Map) on every change so React re-renders.
  const [windowMessages, setWindowMessages] = useState<Map<number, DisplayMessage>>(new Map());

  // Monotonic id: bumped on every loadSession so in-flight fetches for a
  // previous session are ignored when they resolve.
  const loadIdRef = useRef(0);
  const pathRef = useRef("");
  const loadedPagesRef = useRef<Set<number>>(new Set());
  const inflightPagesRef = useRef<Set<number>>(new Set());
  const lastRangeRef = useRef({ start: 0, end: 0 });

  /** Apply the session-level fields returned by any load_session call. */
  const applySessionFields = useCallback((res: LoadResult) => {
    setState((prev) => ({
      ...prev,
      count: res.count,
      roles: res.roles,
      contextTokens: res.context_tokens,
      teams: res.teams,
      ongoing: res.ongoing,
      sessionTotals: res.session_totals,
      meta: res.meta,
    }));
  }, []);

  /** Evict pages outside the keep band. Runs on every `ensureRange` call
   * regardless of whether a fetch is needed — eviction must not wait on a
   * fetch happening to run, or pages left over from a wider prefetch (or from
   * scrolling back and forth within an already-cached region) never get
   * cleaned up and memory only grows. */
  const evictOutsideKeepBand = useCallback((keepFirstPage: number, keepLastPage: number) => {
    setWindowMessages((prev) => {
      let changed = false;
      const next = new Map(prev);
      for (const idx of next.keys()) {
        const p = pageOf(idx);
        if (p < keepFirstPage || p > keepLastPage) {
          next.delete(idx);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
    const survivors = new Set<number>();
    for (const p of loadedPagesRef.current) {
      if (p >= keepFirstPage && p <= keepLastPage) survivors.add(p);
    }
    loadedPagesRef.current = survivors;
  }, []);

  /** Insert a freshly fetched page's messages into the store. */
  const insertWindow = useCallback((res: LoadResult) => {
    setWindowMessages((prev) => {
      const next = new Map(prev);
      res.messages.forEach((msg, i) => next.set(res.start + i, msg));
      return next;
    });
  }, []);

  /** Drop every loaded page. Called when leaving the list view (e.g. opening
   * Detail) so the list's data doesn't sit in memory while a different view
   * is showing — coming back re-fetches the visible window fresh instead. */
  const clearWindow = useCallback(() => {
    setWindowMessages(new Map());
    loadedPagesRef.current = new Set();
  }, []);

  /** Ensure the message bodies covering [start, end) are loaded, fetching any
   * missing pages and evicting pages far from the range. */
  const ensureRange = useCallback(
    async (start: number, end: number) => {
      const path = pathRef.current;
      if (!path) return;
      lastRangeRef.current = { start, end };

      const firstPage = pageOf(Math.max(0, start));
      const lastPage = pageOf(Math.max(start, end - 1));
      const keepFirst = Math.max(0, firstPage - KEEP_MARGIN_PAGES);
      const keepLast = lastPage + KEEP_MARGIN_PAGES;

      // Shrink the store to the current keep band immediately — this must not
      // wait on a fetch, since the pages this range needs may already be cached.
      evictOutsideKeepBand(keepFirst, keepLast);

      // Collect the pages that still need fetching, then fetch them together.
      const missing: number[] = [];
      for (let page = firstPage; page <= lastPage; page++) {
        if (loadedPagesRef.current.has(page) || inflightPagesRef.current.has(page)) continue;
        inflightPagesRef.current.add(page);
        missing.push(page);
      }

      await Promise.all(
        missing.map(async (page) => {
          const myLoadId = loadIdRef.current;
          try {
            const res = await invoke<LoadResult>("load_session", {
              path,
              start: page * PAGE_SIZE,
              limit: PAGE_SIZE,
            });
            if (myLoadId !== loadIdRef.current) return; // session changed mid-fetch
            loadedPagesRef.current.add(page);
            applySessionFields(res);
            insertWindow(res);
          } catch (err) {
            console.error("Failed to fetch message window:", err);
          } finally {
            inflightPagesRef.current.delete(page);
          }
        }),
      );
    },
    [applySessionFields, evictOutsideKeepBand, insertWindow],
  );

  const getMessage = useCallback(
    (index: number): DisplayMessage | undefined => windowMessages.get(index),
    [windowMessages],
  );

  const loadSession = useCallback(
    async (path: string) => {
      const myLoadId = ++loadIdRef.current;
      pathRef.current = path;
      loadedPagesRef.current = new Set();
      inflightPagesRef.current = new Set();
      lastRangeRef.current = { start: 0, end: 0 };
      setWindowMessages(new Map());
      setState({ ...emptyState, sessionPath: path, loading: true });

      try {
        // Stop watching the previous session.
        try {
          await invoke<void>("unwatch_session");
        } catch {
          // ignore if nothing was being watched
        }

        // Metadata-only fetch (limit 0) to learn the count and index.
        const meta = await invoke<LoadResult>("load_session", { path, start: 0, limit: 0 });
        if (myLoadId !== loadIdRef.current) return;

        let gitInfo: GitInfo | null = null;
        if (meta.meta.cwd) {
          try {
            gitInfo = await invoke<GitInfo>("get_git_info", { cwd: meta.meta.cwd });
          } catch {
            // git info is optional
          }
        }
        if (myLoadId !== loadIdRef.current) return;

        // Keep `loading` true until the tail window is primed, so the list
        // doesn't flash placeholders on open.
        setState({
          count: meta.count,
          roles: meta.roles,
          contextTokens: meta.context_tokens,
          teams: meta.teams,
          ongoing: meta.ongoing,
          meta: meta.meta,
          sessionTotals: meta.session_totals,
          sessionPath: path,
          gitInfo,
          debugEntries: [],
          loading: true,
        });

        // Prime the tail window (the view opens at the newest message).
        if (meta.count > 0) {
          await ensureRange(Math.max(0, meta.count - PAGE_SIZE), meta.count);
        }
        if (myLoadId !== loadIdRef.current) return;
        setState((prev) => ({ ...prev, loading: false }));

        // Start watching for updates.
        try {
          await invoke<void>("watch_session", { path });
        } catch {
          // watcher is optional
        }
      } catch (err) {
        console.error("Failed to load session:", err);
        if (myLoadId === loadIdRef.current) {
          setState((prev) => ({ ...prev, loading: false }));
        }
      }
    },
    [ensureRange],
  );

  /** Fetch the full (heavy-body) message at `index` for the detail view. List
   * windows carry lightened messages (no tool bodies), so the detail view pulls
   * the full one on demand. Returns undefined if the session changed or errored. */
  const loadFullMessage = useCallback(
    async (index: number): Promise<DisplayMessage | undefined> => {
      const path = pathRef.current;
      if (!path) return undefined;
      const myLoadId = loadIdRef.current;
      try {
        const msg = await invoke<DisplayMessage | null>("load_message", { path, index });
        if (myLoadId !== loadIdRef.current) return undefined;
        return msg ?? undefined;
      } catch (err) {
        console.error("Failed to load message:", err);
        return undefined;
      }
    },
    [],
  );

  const loadDebugLog = useCallback(async (sessionPath: string) => {
    try {
      const entries = await invoke<DebugEntry[]>("get_debug_log", { sessionPath });
      setState((prev) => ({ ...prev, debugEntries: entries }));
    } catch (err) {
      console.error("Failed to load debug log:", err);
    }
  }, []);

  // Live updates: the watcher emits a lightweight signal (no bodies). Update the
  // session-level fields, then invalidate the visible window so the (possibly
  // streamed) bodies are refetched. Growth in `count` makes the virtualized list
  // request the new tail via ensureRange on its own.
  useTauriEvent<SessionUpdateSignal>("session-update", (payload) => {
    setState((prev) => ({
      ...prev,
      count: payload.count,
      roles: payload.roles,
      contextTokens: payload.context_tokens,
      teams: payload.teams,
      ongoing: payload.ongoing,
      sessionTotals: payload.session_totals,
      meta: {
        ...prev.meta,
        permission_mode: payload.permission_mode || prev.meta.permission_mode,
      },
    }));
    // Force a refetch of the currently visible pages (content may have changed
    // in place without the count growing, e.g. a streaming final message).
    const { start, end } = lastRangeRef.current;
    const firstPage = pageOf(Math.max(0, start));
    const lastPage = pageOf(Math.max(start, end - 1));
    for (let p = firstPage; p <= lastPage; p++) loadedPagesRef.current.delete(p);
    void ensureRange(start, end);
  });

  // Cleanup on unmount.
  useEffect(() => {
    return () => {
      invoke<void>("unwatch_session").catch(() => {});
    };
  }, []);

  return {
    ...state,
    getMessage,
    ensureRange,
    clearWindow,
    loadFullMessage,
    loadSession,
    loadDebugLog,
  };
}
