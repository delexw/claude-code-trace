// Format utilities — ported from format.go

import type { DisplayMessage } from "../types";

/**
 * Turns "claude-opus-4-6" into "opus4.6".
 */
export function shortModel(m: string): string {
  m = m.replace(/^claude-/, "");
  const dashIdx = m.indexOf("-");
  if (dashIdx === -1) return m;

  const family = m.slice(0, dashIdx);
  const rest = m.slice(dashIdx + 1);
  // Keep major-minor only, drop patch/build metadata
  const vParts = rest.split("-");
  let version = vParts[0];
  if (vParts.length >= 2) {
    version = vParts[0] + "." + vParts[1];
  }
  return family + version;
}

/**
 * Formats a token count: 1234 -> "1.2k", 123456 -> "123.5k", 1234567 -> "1.2M"
 */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

/**
 * Formats milliseconds into human-readable duration.
 */
export function formatDuration(ms: number): string {
  const secs = ms / 1000;
  if (secs >= 60) {
    const mins = Math.floor(secs / 60);
    const rem = Math.floor(secs % 60);
    return `${mins}m ${rem}s`;
  }
  if (secs >= 10) return `${Math.round(secs)}s`;
  return `${secs.toFixed(1)}s`;
}

/**
 * Formats a session ID for compact display.
 * Standard UUIDs show only first 8 chars; renamed sessions show up to 20.
 */
export function formatSessionName(id: string): string {
  if (
    id.length === 36 &&
    id[8] === "-" &&
    id[13] === "-" &&
    id[18] === "-" &&
    id[23] === "-"
  ) {
    return id.slice(0, 8);
  }
  return id.length > 20 ? id.slice(0, 20) + "\u2026" : id;
}

/**
 * Returns the project display name from a cwd path.
 * Extracts the last path segment.
 */
export function shortPath(cwd: string, _gitBranch?: string): string {
  if (!cwd) return "";
  const parts = cwd.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? cwd;
}

/**
 * Returns a human-readable label for a permission mode.
 */
export function shortMode(mode: string): string {
  switch (mode) {
    case "default":
      return "default";
    case "acceptEdits":
      return "auto-edit";
    case "bypassPermissions":
      return "yolo";
    case "plan":
      return "plan";
    default:
      return mode;
  }
}

/**
 * Returns context window usage percentage (0-100).
 * Returns -1 if no usage data is available.
 */
export function contextPercent(msgs: DisplayMessage[]): number {
  const contextWindowSize = 200_000;
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === "claude" && msgs[i].context_tokens > 0) {
      const pct = Math.floor(
        (msgs[i].context_tokens * 100) / contextWindowSize
      );
      return Math.min(pct, 100);
    }
  }
  return -1;
}

/**
 * Formats a timestamp string for display.
 */
export function formatTime(ts: string): string {
  if (!ts) return "";
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return "";
    return d.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "";
  }
}

/**
 * Groups sessions by date category.
 */
export function groupByDate<T extends { mod_time: string }>(
  items: T[]
): { category: string; items: T[] }[] {
  const now = new Date();
  const todayStart = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate()
  );
  const yesterdayStart = new Date(todayStart.getTime() - 86400000);
  const weekStart = new Date(todayStart.getTime() - 7 * 86400000);
  const monthStart = new Date(todayStart.getTime() - 30 * 86400000);

  const groups: Record<string, T[]> = {};
  const order = ["Today", "Yesterday", "This Week", "This Month", "Older"];

  for (const item of items) {
    const d = new Date(item.mod_time);
    let cat: string;
    if (d >= todayStart) cat = "Today";
    else if (d >= yesterdayStart) cat = "Yesterday";
    else if (d >= weekStart) cat = "This Week";
    else if (d >= monthStart) cat = "This Month";
    else cat = "Older";

    (groups[cat] ??= []).push(item);
  }

  return order
    .filter((cat) => groups[cat]?.length)
    .map((category) => ({ category, items: groups[category] }));
}

/**
 * Truncates text to maxLen, appending ellipsis if needed.
 */
export function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen - 1) + "\u2026";
}

/**
 * Returns the first non-empty line of text.
 */
export function firstLine(text: string): string {
  const idx = text.indexOf("\n");
  return idx === -1 ? text : text.slice(0, idx);
}
