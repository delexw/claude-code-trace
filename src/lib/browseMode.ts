/** Which list the picker browses: Claude sessions, Antigravity conversations,
 *  or both. Persisted in localStorage so it survives reloads. Defaults to
 *  `all` so the picker looks the same as it did before the mode existed. */
import type { BrowseMode } from "../types";

export const BROWSE_MODE_KEY = "cct.browseMode";
const DEFAULT_BROWSE_MODE: BrowseMode = "all";

/** The three ways the list can be read. Rendered by `ViewToolbar`, which owns
 *  it because a browse mode is a view-level choice, not a list-level one. */
export const BROWSE_MODES: { id: BrowseMode; label: string; title: string }[] = [
  { id: "claude", label: "Claude", title: "Claude Code sessions only" },
  { id: "antigravity", label: "Antigravity", title: "Antigravity conversations only" },
  { id: "all", label: "All", title: "Both sources in one list" },
];

/** Offered while Antigravity is switched off — no empty promises. */
export const CLAUDE_ONLY_MODES = BROWSE_MODES.slice(0, 1);

/** A stored mode can outlive the setting that allowed it (Antigravity turned
 *  off while "antigravity" was selected), so clamp before it reaches the UI. */
export function selectableModes(antigravityEnabled: boolean) {
  return antigravityEnabled ? BROWSE_MODES : CLAUDE_ONLY_MODES;
}

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
