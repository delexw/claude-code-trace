// Format utilities — ported from format.go
// Pure shared utilities are re-exported from shared/format.ts.

import type { DisplayMessage } from "../types";
import { transformInlineJson } from "../../shared/format";
export { formatTokens, transformInlineJson } from "../../shared/format";
export { computeEditDiff } from "../../shared/diff";
export type { DiffLine, DiffSegment, DiffLineKind } from "../../shared/diff";

/**
 * Turns "claude-opus-4-6" into "opus4.6".
 * Strips bracket context suffixes (e.g. "[1m]", "[1M]") and 8-digit date
 * components so "claude-fable-5-20261001[1m]" becomes "fable5".
 */
export function shortModel(m: string): string {
  // Strip bracket suffix (e.g., "[1m]", "[1M]") before normalizing.
  const bracketIdx = m.indexOf("[");
  if (bracketIdx !== -1) m = m.slice(0, bracketIdx);

  m = m.replace(/^claude-/, "");
  const dashIdx = m.indexOf("-");
  if (dashIdx === -1) return m;

  const family = m.slice(0, dashIdx);
  const rest = m.slice(dashIdx + 1);
  // Keep major-minor only; skip pure 8-digit date components (YYYYMMDD).
  const vParts = rest.split("-").filter((p) => !(p.length === 8 && /^\d+$/.test(p)));
  const version = vParts.slice(0, 2).join(".");
  return family + version;
}

// Claude API pricing per million tokens (USD).
// Model-specific rates; we pick by the model string prefix.
const MODEL_PRICING: {
  prefix: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}[] = [
  { prefix: "opus", input: 5, output: 25, cacheRead: 0.5, cacheWrite: 6.25 },
  { prefix: "sonnet", input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 },
  { prefix: "haiku", input: 1, output: 5, cacheRead: 0.1, cacheWrite: 1.25 },
];

function pricingForModel(model: string) {
  const m = model.toLowerCase();
  return MODEL_PRICING.find((p) => m.includes(p.prefix)) ?? MODEL_PRICING[1]; // default sonnet
}

/**
 * Estimates USD cost from token breakdown and model string.
 */
export function estimateCost(
  input: number,
  output: number,
  cacheRead: number,
  cacheWrite: number,
  model: string,
): number {
  const p = pricingForModel(model);
  return (
    (input * p.input + output * p.output + cacheRead * p.cacheRead + cacheWrite * p.cacheWrite) /
    1_000_000
  );
}

/**
 * Formats a dollar amount: 0.0023 -> "0.00", 1.23 -> "1.23", 12.5 -> "12.50"
 */
export function formatCost(usd: number): string {
  return usd.toFixed(2);
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

// shortPath, projectKey, projectDisplayName — from shared/format.ts
export { shortPath, projectKey, projectDisplayName } from "../../shared/format";

/**
 * Returns true for permission modes that represent the default (no-special-permissions) state.
 * Both "default" (pre-v2.1.200) and "manual" (v2.1.200+) are the default mode — no badge shown.
 */
export function isDefaultMode(mode: string): boolean {
  return mode === "default" || mode === "manual";
}

/**
 * Returns a human-readable label for a permission mode.
 */
export function shortMode(mode: string): string {
  switch (mode) {
    case "default":
      return "default";
    case "manual":
      return "manual";
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
/** Context-window fill percent from a raw token count. Returns -1 (hidden) when
 * there is no usage (tokens <= 0). Uses the 1M default context window. */
export function contextPercentFromTokens(tokens: number): number {
  if (tokens <= 0) return -1;
  const contextWindowSize = 1_000_000;
  return Math.min(Math.floor((tokens * 100) / contextWindowSize), 100);
}

export function contextPercent(msgs: DisplayMessage[]): number {
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === "claude" && msgs[i].context_tokens > 0) {
      return contextPercentFromTokens(msgs[i].context_tokens);
    }
  }
  return -1;
}

/**
 * Formats a timestamp as yyyy-mm-dd hh:mm:ss.
 */
export function formatExactTime(ts: string): string {
  if (!ts) return "";
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return "";
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    const hh = String(d.getHours()).padStart(2, "0");
    const mi = String(d.getMinutes()).padStart(2, "0");
    const ss = String(d.getSeconds()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd} ${hh}:${mi}:${ss}`;
  } catch {
    return "";
  }
}

/**
 * Groups sessions by date category, sorted by mod_time descending within each group.
 */
export function groupByDate<T extends { mod_time: string }>(
  items: T[],
): { category: string; items: T[] }[] {
  const now = new Date();
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
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

  // Sort each group by mod_time descending (most recent first)
  for (const cat of order) {
    if (groups[cat]) {
      groups[cat].sort((a, b) => new Date(b.mod_time).getTime() - new Date(a.mod_time).getTime());
    }
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

/**
 * Returns the first n lines of text.
 */
export function firstNLines(text: string, n: number): string {
  const lines = text.split("\n");
  return lines.slice(0, n).join("\n");
}

/**
 * Pretty-prints a JSON string. Returns the original string on parse failure.
 */
export function formatJson(input: string): string {
  try {
    return JSON.stringify(JSON.parse(input), null, 2);
  } catch {
    return input;
  }
}

export interface EditDiffData {
  filePath: string;
  oldLines: string[];
  newLines: string[];
  replaceAll: boolean;
}

export function parseEditInput(toolInput: string): EditDiffData | null {
  try {
    const parsed = JSON.parse(toolInput);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      typeof parsed.file_path !== "string" ||
      typeof parsed.old_string !== "string" ||
      typeof parsed.new_string !== "string"
    ) {
      return null;
    }
    return {
      filePath: parsed.file_path,
      oldLines: parsed.old_string.split("\n"),
      newLines: parsed.new_string.split("\n"),
      replaceAll: parsed.replace_all === true,
    };
  } catch {
    return null;
  }
}

/**
 * Wraps bare JSON objects/arrays in ```json code fences for ReactMarkdown.
 * Delegates detection to the shared transformInlineJson utility.
 */
export function fenceInlineJson(text: string): string {
  return transformInlineJson(text, (prefix, formatted) => {
    const fence = "```json\n" + formatted + "\n```";
    return prefix ? prefix + "\n" + fence : fence;
  });
}
