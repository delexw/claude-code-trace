import type { RouteTable } from "../../lib/routeTypes";

export const codexRoutes: RouteTable = {
  prepare_codex_efficiency_payload: {
    method: "POST",
    path: "/api/codex/efficiency/prepare",
    body: (args) => ({ path: args.path, payloadMode: args.payloadMode }),
  },
  list_codex_sessions: { path: "/api/codex/sessions" },
  get_codex_settings: { path: "/api/codex/settings" },
  set_codex_root: {
    method: "POST",
    path: "/api/codex/root",
    body: (args) => ({ path: args.path ?? null }),
  },
  load_codex_session: {
    method: "POST",
    path: "/api/codex/session",
    body: (args) => ({ path: args.path, start: args.start, limit: args.limit }),
  },
};
