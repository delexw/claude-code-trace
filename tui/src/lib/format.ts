/** TUI-specific formatting utilities. Shared format functions come from shared/format.ts. */

export {
  formatTokens,
  formatCost,
  formatDuration,
  truncate,
  timeAgo,
} from "../../../shared/format.js";

import { colors, getModelColor as _getModelColor } from "./theme.js";

/** Turns "claude-opus-4-6" into "opus4.6". */
export function shortModel(m: string): string {
  const s = m.replace(/^claude-/, "");
  const dashIdx = s.indexOf("-");
  if (dashIdx === -1) return s;
  const family = s.slice(0, dashIdx);
  const rest = s.slice(dashIdx + 1);
  const vParts = rest.split("-");
  let version = vParts[0];
  if (vParts.length >= 2) {
    version = vParts[0] + "." + vParts[1];
  }
  return family + version;
}

/** Returns hex color for a model string (matches web theme). */
export function modelColor(m: string): string {
  return _getModelColor(m);
}

/** Returns hex color for a message role (matches web theme). */
export function roleColor(role: string): string {
  switch (role) {
    case "claude":
      return colors.textSecondary;
    case "user":
      return colors.accent;
    case "system":
      return colors.textMuted;
    default:
      return colors.textPrimary;
  }
}

/** Returns a unicode icon for a message role (matches web's VscAccount/Claude/VscTerminal). */
export function roleIcon(role: string): string {
  switch (role) {
    case "claude":
      return "✦";
    case "user":
      return "●";
    case "system":
      return "$";
    default:
      return " ";
  }
}

/** Returns the first non-empty line of text. */
export function firstLine(text: string): string {
  const idx = text.indexOf("\n");
  return idx === -1 ? text : text.slice(0, idx);
}

/** Pretty-prints a JSON string. Returns original on parse failure. */
export function formatJson(input: string): string {
  try {
    return JSON.stringify(JSON.parse(input), null, 2);
  } catch {
    return input;
  }
}
