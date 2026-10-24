/**
 * Safe wrapper around Tauri's `invoke`.
 *
 * In the Tauri webview it delegates to the real IPC bridge.
 * In a plain browser it calls the Rust backend's HTTP API.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { isTauri } from "./isTauri";
import { API_BASE } from "./config";
import { authHeaders } from "./apiToken";
import type { RouteTable } from "./routeTypes";
import { PLUGIN_ROUTES } from "../plugins/routes";

// ---------------------------------------------------------------------------
// Route map — add new commands here without touching invoke logic (OCP).
// ---------------------------------------------------------------------------

/** Declarative mapping from the app's own Tauri command names to HTTP endpoints. */
const coreRoutes: RouteTable = {
  get_settings: { path: "/api/settings" },
  set_plugin_enabled: {
    method: "POST",
    path: "/api/plugins/enabled",
    body: (a) => ({ id: a.id, enabled: a.enabled }),
  },
  get_analytics_settings: { path: "/api/analytics/settings" },
  set_analytics_settings: {
    method: "POST",
    path: "/api/analytics/settings",
    body: (a) => ({
      defaultPayloadMode: a.defaultPayloadMode,
      recommendationProvider: a.recommendationProvider,
    }),
  },
  test_jev_connection: { method: "POST", path: "/api/analytics/jev/test" },
  test_recommendation_provider: {
    method: "POST",
    path: "/api/analytics/recommendation/test",
    body: (a) => ({ provider: a.provider }),
  },
  prepare_session_efficiency_payload: {
    method: "POST",
    path: "/api/efficiency/prepare",
    body: (a) => ({ path: a.path, payloadMode: a.payloadMode }),
  },
  start_session_efficiency_analysis: {
    method: "POST",
    path: "/api/efficiency/start",
    body: (a) => ({ path: a.path, payload: a.payload }),
  },
  list_efficiency_analysis_jobs: { path: "/api/efficiency/jobs" },
  cancel_efficiency_analysis: {
    method: "POST",
    path: (a) => `/api/efficiency/job/${encodeURIComponent(String(a.analysisId ?? ""))}/cancel`,
  },
  get_session_efficiency: {
    path: (a) => `/api/efficiency/result?path=${encodeURIComponent(String(a.path ?? ""))}`,
  },
  list_efficiency_summaries: { path: "/api/efficiency/summaries" },
  delete_session_efficiency: {
    method: "DELETE",
    path: (a) => `/api/efficiency/result?path=${encodeURIComponent(String(a.path ?? ""))}`,
  },
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
  list_clients: { path: "/api/clients" },
  register_client: {
    method: "POST",
    path: "/api/clients",
    body: (a) => ({ name: String(a.name ?? "") }),
  },
  reissue_client: {
    method: "POST",
    path: (a) => `/api/clients/${encodeURIComponent(String(a.id ?? ""))}/reissue`,
  },
  revoke_client: {
    method: "POST",
    path: (a) => `/api/clients/${encodeURIComponent(String(a.id ?? ""))}/revoke`,
  },
  whoami: { path: "/api/whoami" },
};

/** Every command the web build can call: the app's own, then each plugin's. */
const routes: RouteTable = Object.assign({}, coreRoutes, ...Object.values(PLUGIN_ROUTES));

// ---------------------------------------------------------------------------
// HTTP transport (SRP — only handles fetch, not routing).
// ---------------------------------------------------------------------------

/** The backend rejected the call because this client did not present a valid
 * client credential (HTTP 401). Surfaced by `App` as a top-level banner since
 * every other call — including opening Settings — would fail the same way. */
export class ApiAuthError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ApiAuthError";
  }
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    // The credential header is what makes this an "accepted client" to the
    // backend (see lib/apiToken.ts). In Docker it is empty and the cookie does
    // the job.
    headers: { "Content-Type": "application/json", ...authHeaders(), ...init?.headers },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    const message = body.error ?? res.statusText;
    if (res.status === 401) throw new ApiAuthError(message);
    throw new Error(message);
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
