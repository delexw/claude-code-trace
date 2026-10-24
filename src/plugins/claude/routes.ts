import type { RouteTable } from "../../lib/routeTypes";

/** HTTP endpoints for Claude's commands, used when the app runs in a plain browser. */
export const claudeRoutes: RouteTable = {
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
