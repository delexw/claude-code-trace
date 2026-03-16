/** HTTP API client for the Claude Code Trace backend (port 11423). */

// Types are shared with the main Tauri UI.
export type {
  SessionInfo,
  DisplayMessage,
  DisplayItem,
  LoadResult,
  SessionMeta,
  SessionTotals,
  TeamSnapshot,
  TeamTask,
  DebugEntry,
  GitInfo,
} from "../../shared/types.js";

import type { SessionInfo, LoadResult, DebugEntry, GitInfo } from "../../shared/types.js";

export interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
}

const API = "http://127.0.0.1:11423";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${API}${path}`);
  if (!res.ok) throw new Error(`API ${path}: ${res.statusText}`);
  return res.json() as Promise<T>;
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${API}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`API ${path}: ${res.statusText}`);
  const text = await res.text();
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

export const api = {
  getSettings: () => get<SettingsResponse>("/api/settings"),
  getProjectDirs: () => get<string[]>("/api/project-dirs"),
  discoverSessions: (dirs: string[]) =>
    get<SessionInfo[]>(`/api/sessions?dirs=${encodeURIComponent(dirs.join(","))}`),
  loadSession: (path: string) => post<LoadResult>("/api/session/load", { path }),
  watchSession: (path: string) => post<void>("/api/session/watch", { path }),
  unwatchSession: () => post<void>("/api/session/unwatch"),
  getDebugLog: (sessionPath: string) =>
    get<DebugEntry[]>(`/api/debug-log?path=${encodeURIComponent(sessionPath)}`),
  getGitInfo: (cwd: string) => get<GitInfo>(`/api/git-info?cwd=${encodeURIComponent(cwd)}`),
};
