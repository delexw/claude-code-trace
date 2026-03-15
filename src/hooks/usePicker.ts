import { useState, useEffect, useCallback } from "react";
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

  const discoverSessions = useCallback(async (projectDirs: string[]) => {
    setState((prev) => ({ ...prev, loading: true }));
    try {
      const sessions = await invoke<SessionInfo[]>("discover_sessions", {
        projectDirs,
      });
      setState((prev) => ({ ...prev, sessions, loading: false }));

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
  }, []);

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

  // Listen for picker-refresh events (backend already applies staleness)
  useTauriEvent<{ sessions: SessionInfo[] }>("picker-refresh", (payload) => {
    setState((prev) => ({
      ...prev,
      sessions: payload.sessions,
    }));
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
    updateSessionOngoing,
  };
}
