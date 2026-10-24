import type { RouteTable } from "../../lib/routeTypes";

/** HTTP endpoints for Antigravity's commands, used when the app runs in a plain browser. */
export const antigravityRoutes: RouteTable = {
  list_antigravity_folders: { path: "/api/antigravity/folders" },
  get_antigravity_settings: { path: "/api/antigravity/settings" },
  set_antigravity_root: {
    method: "POST",
    path: "/api/antigravity/root",
    body: (a) => ({ path: a.path ?? null }),
  },
  load_antigravity_session: {
    method: "POST",
    path: "/api/antigravity/session",
    body: (a) => ({ path: a.path, start: a.start, limit: a.limit }),
  },
};
