/** Which list the picker browses: Claude sessions, Antigravity conversations,
 *  or both. Persisted in localStorage so it survives reloads. Defaults to
 *  `all` so the picker looks the same as it did before the mode existed. */
import type { BrowseMode } from "../types";

export const BROWSE_MODE_KEY = "cct.browseMode";
const DEFAULT_BROWSE_MODE: BrowseMode = "all";

const VALID = new Set<BrowseMode>(["claude", "antigravity", "all"]);

export function loadBrowseMode(): BrowseMode {
  if (typeof localStorage === "undefined") return DEFAULT_BROWSE_MODE;
  const raw = localStorage.getItem(BROWSE_MODE_KEY);
  if (raw === null) return DEFAULT_BROWSE_MODE;
  // A value written by an older build (or hand-edited) must not put the
  // picker into a mode it has no name for.
  return VALID.has(raw as BrowseMode) ? (raw as BrowseMode) : DEFAULT_BROWSE_MODE;
}

export function saveBrowseMode(mode: BrowseMode): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(BROWSE_MODE_KEY, mode);
  } catch {
    // Storage may be full or disabled; the in-memory selection still applies.
  }
}
