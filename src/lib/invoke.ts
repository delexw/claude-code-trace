/**
 * Safe wrapper around Tauri's `invoke`.
 *
 * In the Tauri webview it delegates to the real IPC bridge.
 * In a plain browser it calls the Rust backend's HTTP API.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { isTauri } from "./isTauri";
import { API_BASE } from "./config";

// ---------------------------------------------------------------------------
// Route map — add new commands here without touching invoke logic (OCP).
// ---------------------------------------------------------------------------

interface Route {
  method?: "POST";
  path: string | ((args: Record<string, unknown>) => string);
  body?: (args: Record<string, unknown>) => unknown;
}

/** Declarative mapping from Tauri command names to HTTP endpoints. */
const routes: Record<string, Route> = {
  get_settings: { path: "/api/settings" },
  set_projects_dir: {
    method: "POST",
    path: "/api/settings/dir",
    body: (a) => ({ path: a.path ?? null }),
  },
  list_wsl_distros: { path: "/api/wsl/distros" },
  set_wsl_distros: {
    method: "POST",
    path: "/api/wsl/distros",
    body: (a) => ({ distros: (a.distros as string[]) ?? [] }),
  },
  set_allowed_origins: {
    method: "POST",
    path: "/api/settings/origins",
    body: (a) => ({ origins: (a.origins as string[]) ?? [] }),
  },
  get_project_dirs: { path: "/api/project-dirs" },
  discover_sessions: {
    method: "POST",
    path: "/api/sessions",
    body: (a) => ({ dirs: (a.projectDirs as string[]) ?? [] }),
  },
  load_session: {
    method: "POST",
    path: "/api/session/load",
    body: (a) => ({ path: a.path, start: a.start, limit: a.limit }),
  },
  load_message: {
    method: "POST",
    path: "/api/session/message",
    body: (a) => ({ path: a.path, index: a.index }),
  },
  get_session_meta: {
    path: (a) => `/api/session/meta?path=${encodeURIComponent(String(a.path ?? ""))}`,
  },
  watch_session: {
    method: "POST",
    path: "/api/session/watch",
    body: (a) => ({ path: a.path }),
  },
  unwatch_session: { method: "POST", path: "/api/session/unwatch" },
  watch_picker: {
    method: "POST",
    path: "/api/picker/watch",
    body: (a) => ({ projectDirs: a.projectDirs }),
  },
  unwatch_picker: { method: "POST", path: "/api/picker/unwatch" },
  get_git_info: {
    path: (a) => `/api/git-info?cwd=${encodeURIComponent(String(a.cwd ?? ""))}`,
  },
  get_debug_log: {
    path: (a) => {
      const params = new URLSearchParams();
      params.set("path", String(a.sessionPath ?? ""));
      if (a.minLevel) params.set("minLevel", String(a.minLevel));
      if (a.filterText) params.set("filterText", String(a.filterText));
      return `/api/debug-log?${params}`;
    },
  },
  focus_session_window: {
    method: "POST",
    path: "/api/focus",
    body: (a) => ({ sessionId: a.sessionId }),
  },
};

// ---------------------------------------------------------------------------
// HTTP transport (SRP — only handles fetch, not routing).
// ---------------------------------------------------------------------------

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: { "Content-Type": "application/json", ...init?.headers },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error ?? res.statusText);
  }
  const text = await res.text();
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

/** Resolve a route to a fetch call. */
async function httpInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const route = routes[cmd];
  if (!route) {
    throw new Error(`[web] Unknown command "${cmd}"`);
  }
  const a = args ?? {};
  const path = typeof route.path === "function" ? route.path(a) : route.path;
  const init: RequestInit = {};
  if (route.method) init.method = route.method;
  if (route.body) init.body = JSON.stringify(route.body(a));
  return fetchJson<T>(`${API_BASE}${path}`, init);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

let inFlightCount = 0;

/** Number of `invoke` calls currently awaiting a response. Reloading the
 * webview while a Tauri IPC call is in flight can crash it on macOS — callers
 * that need to reload (see lib/webviewRecycle.ts) check this first. */
export function inFlightInvokeCount(): number {
  return inFlightCount;
}

/**
 * Drop-in replacement for `import { invoke } from "@tauri-apps/api/core"`.
 * Works in both Tauri and plain-browser environments.
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  inFlightCount++;
  try {
    if (isTauri) {
      return await tauriInvoke<T>(cmd, args);
    }
    return await httpInvoke<T>(cmd, args);
  } finally {
    inFlightCount--;
  }
}
