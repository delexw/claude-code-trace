import type { PluginStatus } from "../types";
import type { SourceContext, SourcePlugin, SourceRuntime } from "./types";
import { claudePlugin } from "./claude";

/** Every session-source plugin, in display order. Adding a plugin = adding it here. */
export const PLUGINS: readonly SourcePlugin[] = [claudePlugin];

/**
 * Whether `plugin` is on. A plugin that cannot be disabled is always on; any other
 * plugin is off until the backend's settings say otherwise.
 */
export function isPluginEnabled(plugin: SourcePlugin, statuses: PluginStatus[] | undefined) {
  if (!plugin.canDisable) return true;
  return statuses?.find((status) => status.id === plugin.id)?.enabled ?? false;
}

/**
 * Run every plugin's `useSource`, in order. `plugins` must be the same module constant
 * on every render, so the same hooks run in the same order.
 */
export function useSourceRuntimes(
  plugins: readonly SourcePlugin[],
  contextFor: (plugin: SourcePlugin) => SourceContext,
): SourceRuntime[] {
  return plugins.map((plugin) => plugin.useSource(contextFor(plugin)));
}
