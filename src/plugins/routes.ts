import type { RouteTable } from "../lib/routeTypes";
import { antigravityRoutes } from "./antigravity/routes";
import { claudeRoutes } from "./claude/routes";

/**
 * Each plugin's web-mode routes, keyed by plugin id. Kept apart from the plugin
 * registry so `lib/invoke` can read it without importing the plugins' React code.
 */
export const PLUGIN_ROUTES: Record<string, RouteTable> = {
  claude: claudeRoutes,
  antigravity: antigravityRoutes,
};
