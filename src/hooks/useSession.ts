import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
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

interface SessionState {
  messages: DisplayMessage[];
  teams: TeamSnapshot[];
  ongoing: boolean;
  meta: SessionMeta;
  sessionTotals: SessionTotals;
  sessionPath: string;
  gitInfo: GitInfo | null;
  debugEntries: DebugEntry[];
  loading: boolean;
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

export function useSession() {
  const [state, setState] = useState<SessionState>({
    messages: [],
    teams: [],
    ongoing: false,
    meta: emptyMeta,
    sessionTotals: emptyTotals,
    sessionPath: "",
    gitInfo: null,
    debugEntries: [],
    loading: false,
  });

  const loadSession = useCallback(async (path: string) => {
    setState((prev) => ({ ...prev, loading: true }));
    try {
      // Stop watching previous session
      try {
        await invoke<void>("unwatch_session");
      } catch {
        // ignore if no session was being watched
      }

      const result = await invoke<LoadResult>("load_session", { path });

      // Extract git info from session cwd
      let gitInfo: GitInfo | null = null;
      if (result.meta.cwd) {
        try {
          gitInfo = await invoke<GitInfo>("get_git_info", {
            cwd: result.meta.cwd,
          });
        } catch {
          // git info is optional
        }
      }

      setState({
        messages: result.messages,
        teams: result.teams,
        ongoing: result.ongoing,
        meta: result.meta,
        sessionTotals: result.session_totals,
        sessionPath: path,
        gitInfo,
        debugEntries: [],
        loading: false,
      });

      // Start watching for updates
      try {
        await invoke<void>("watch_session", { path });
      } catch {
        // watcher is optional
      }
    } catch (err) {
      console.error("Failed to load session:", err);
      setState((prev) => ({ ...prev, loading: false }));
    }
  }, []);

  const loadDebugLog = useCallback(async (sessionPath: string) => {
    try {
      const entries = await invoke<DebugEntry[]>("get_debug_log", {
        sessionPath,
      });
      setState((prev) => ({ ...prev, debugEntries: entries }));
    } catch (err) {
      console.error("Failed to load debug log:", err);
    }
  }, []);

  // Listen for session-update events
  useTauriEvent<{
    messages: DisplayMessage[];
    teams: TeamSnapshot[];
    ongoing: boolean;
    permission_mode: string;
    session_totals: SessionTotals;
  }>("session-update", (payload) => {
    setState((prev) => ({
      ...prev,
      messages: payload.messages,
      teams: payload.teams,
      ongoing: payload.ongoing,
      sessionTotals: payload.session_totals,
      meta: {
        ...prev.meta,
        permission_mode: payload.permission_mode || prev.meta.permission_mode,
      },
    }));
  });

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      invoke<void>("unwatch_session").catch(() => {});
    };
  }, []);

  return {
    ...state,
    loadSession,
    loadDebugLog,
  };
}
