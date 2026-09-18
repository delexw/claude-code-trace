import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "../lib/invoke";
import type { IndexProgress, SessionInfo } from "../types";
import { useTauriEvent } from "./useTauriEvent";
import { projectKey } from "../../shared/format";
import { resolveForkRoot } from "../../shared/projectTree";

interface PickerState {
  sessions: SessionInfo[];
  loading: boolean;
  searchQuery: string;
  /** How far the backend has got through the project directories. */
  index: IndexProgress;
}

/** Nothing is being walked, so the bar stays hidden. */
const indexNotStarted: IndexProgress = {
  files_read: 0,
  total_files: 0,
  bytes_read: 0,
  total_bytes: 0,
  done: true,
};

/**
 * Whether two progress reports say the same thing. Bytes are part of it: a single huge
 * session file reports its bytes as it is read while the file count stands still, and
 * dropping those would freeze the bar for as long as that file takes.
 */
function sameProgress(a: IndexProgress, b: IndexProgress): boolean {
  return (
    a.files_read === b.files_read &&
    a.total_files === b.total_files &&
    a.bytes_read === b.bytes_read &&
    a.total_bytes === b.total_bytes &&
    a.done === b.done
  );
}

export function usePicker(selectedProject: string | null = null) {
  const [state, setState] = useState<PickerState>({
    sessions: [],
    loading: false,
    searchQuery: "",
    index: indexNotStarted,
  });

  // Track the most recent project dirs so picker-refresh signals can re-fetch
  // without needing the caller to re-supply them.
  const projectDirsRef = useRef<string[] | null>(null);

  const fetchSessions = useCallback(async (projectDirs: string[]) => {
    const sessions = await invoke<SessionInfo[]>("discover_sessions", {
      projectDirs,
    });
    setState((prev) => ({ ...prev, sessions, loading: false }));
  }, []);

  const discoverSessions = useCallback(
    async (projectDirs: string[]) => {
      projectDirsRef.current = projectDirs;
      setState((prev) => ({ ...prev, loading: true }));
      try {
        await fetchSessions(projectDirs);

        // Start watching for new sessions
        try {
          await invoke<void>("watch_picker", { projectDirs });
        } catch {
          // watcher is optional
        }
      } catch (err) {
        console.error("Failed to discover sessions:", err);
        setState((prev) => ({ ...prev, loading: false }));
      }
    },
    [fetchSessions],
  );

  /**
   * Re-fetch the session list using the most recently supplied project dirs.
   * Used by the viewport-aware picker to refresh visible cards eagerly,
   * independent of file-system events. Cheap: coalesced by the backend's
   * sessions cache.
   */
  const refresh = useCallback(() => {
    const dirs = projectDirsRef.current;
    if (!dirs) return;
    fetchSessions(dirs).catch((err) => {
      console.error("Failed to refresh sessions:", err);
    });
  }, [fetchSessions]);

  const setSearchQuery = useCallback((query: string) => {
    setState((prev) => ({ ...prev, searchQuery: query }));
  }, []);

  /** Sync a session's ongoing status from the session watcher (more accurate). */
  const updateSessionOngoing = useCallback((path: string, ongoing: boolean) => {
    setState((prev) => {
      const idx = prev.sessions.findIndex((s) => s.path === path);
      if (idx === -1 || prev.sessions[idx].is_ongoing === ongoing) return prev;
      const sessions = [...prev.sessions];
      sessions[idx] = { ...sessions[idx], is_ongoing: ongoing };
      return { ...prev, sessions };
    });
  }, []);

  // The backend emits a lightweight signal (no payload). Re-fetch project dirs
  // first so newly created project folders are discovered, then fetch sessions.
  useTauriEvent<unknown>("picker-refresh", async () => {
    try {
      const freshDirs = await invoke<string[]>("get_project_dirs");
      projectDirsRef.current = freshDirs;
      await fetchSessions(freshDirs);
    } catch {
      // Fallback: use stale dirs so existing sessions still refresh.
      const dirs = projectDirsRef.current;
      if (dirs) fetchSessions(dirs).catch(console.error);
    }
  });

  // How far the backend has got walking the project directories, several times a second
  // while it runs. Only the bar moves on these: the walk sends a `picker-refresh` of its
  // own whenever it has actually read more sessions, so fetching the list here too would
  // put every visible session on the wire four times a second for minutes.
  useTauriEvent<IndexProgress>("index-progress", (progress) => {
    setState((prev) => (sameProgress(prev.index, progress) ? prev : { ...prev, index: progress }));
  });

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      invoke<void>("unwatch_picker").catch(() => {});
    };
  }, []);

  // Filter sessions by search query
  let filteredSessions = state.searchQuery
    ? state.sessions.filter(
        (s) =>
          (s.name?.toLowerCase().includes(state.searchQuery.toLowerCase()) ?? false) ||
          s.first_message.toLowerCase().includes(state.searchQuery.toLowerCase()) ||
          s.session_id.toLowerCase().includes(state.searchQuery.toLowerCase()) ||
          s.model.toLowerCase().includes(state.searchQuery.toLowerCase()),
      )
    : state.sessions;

  // Filter by selected project. Resolved against the full (unfiltered) session list so a
  // forked session — grouped under its fork parent's project in the sidebar tree, see
  // shared/projectTree.ts — is included even when the parent itself was excluded above by
  // the search query.
  if (selectedProject) {
    const sessionsById = new Map(state.sessions.map((s) => [s.session_id, s]));
    filteredSessions = filteredSessions.filter(
      (s) => projectKey(resolveForkRoot(s, sessionsById).path) === selectedProject,
    );
  }

  return {
    sessions: filteredSessions,
    allSessions: state.sessions,
    index: state.index,
    loading: state.loading,
    searchQuery: state.searchQuery,
    setSearchQuery,
    discoverSessions,
    refresh,
    updateSessionOngoing,
  };
}
