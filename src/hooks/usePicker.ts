import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "../lib/invoke";
import type { SessionInfo } from "../types";
import { useTauriEvent } from "./useTauriEvent";

interface PickerState {
  sessions: SessionInfo[];
  loading: boolean;
  searchQuery: string;
}

export function usePicker(selectedProject: string | null = null) {
  const [state, setState] = useState<PickerState>({
    sessions: [],
    loading: false,
    searchQuery: "",
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
          s.first_message.toLowerCase().includes(state.searchQuery.toLowerCase()) ||
          s.session_id.toLowerCase().includes(state.searchQuery.toLowerCase()) ||
          s.model.toLowerCase().includes(state.searchQuery.toLowerCase()),
      )
    : state.sessions;

  // Filter by selected project
  if (selectedProject) {
    filteredSessions = filteredSessions.filter((s) =>
      s.path.replace(/\\/g, "/").includes(`/.claude/projects/${selectedProject}/`),
    );
  }

  return {
    sessions: filteredSessions,
    allSessions: state.sessions,
    loading: state.loading,
    searchQuery: state.searchQuery,
    setSearchQuery,
    discoverSessions,
    refresh,
    updateSessionOngoing,
  };
}
