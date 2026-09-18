/**
 * Pure formatting utilities shared between Tauri UI and TUI.
 * No React, DOM, or framework dependencies.
 */

/** Formats a token count: 1234 -> "1.2k", 1234567 -> "1.2M" */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

/** Formats a byte count: 3_700_000 -> "3.7 MB" */
export function formatBytes(n: number): string {
  if (n >= 1e9) return (n / 1e9).toFixed(1) + " GB";
  if (n >= 1e6) return (n / 1e6).toFixed(1) + " MB";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + " KB";
  return n + " B";
}

/** Formats USD cost: 1.5 -> "$1.50" */
export function formatCost(usd: number): string {
  return "$" + usd.toFixed(2);
}

/** Formats duration: 1500 -> "1.5s", 90000 -> "1m 30s" */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  const rem = Math.floor(s % 60);
  return rem > 0 ? `${m}m ${rem}s` : `${m}m`;
}

/** Truncate string to max length with ellipsis, collapsing newlines. */
export function truncate(s: string, max: number): string {
  const line = s.replace(/\n/g, " ").trim();
  return line.length > max ? line.slice(0, max - 1) + "…" : line;
}

/** Extract the encoded project directory key from a session path. */
export function projectKey(path: string): string {
  const match = path.match(/[/\\]\.claude[/\\]projects[/\\]([^/\\]+)/);
  return match ? match[1] : "unknown";
}

/** Decode a project key to a display name (last path segment). */
export function projectDisplayName(key: string): string {
  const path = key.replace(/^-/, "/").replaceAll("-", "/");
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? key;
}

/** Extract the last path segment. */
export function shortPath(cwd: string): string {
  if (!cwd) return "";
  const parts = cwd.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? cwd;
}

const MIN_JSON_TRANSFORM_LENGTH = 15;

/**
 * Scans each line of text for bare JSON objects/arrays (not already inside a
 * code fence) and replaces them using the provided wrap callback.
 *
 * wrap(prefix, formattedJson) → replacement line
 *   prefix  — text before the JSON blob on the same line (may be empty)
 *   formattedJson — JSON.stringify(parsed, null, 2)
 *
 * Used by both platforms:
 *   - GUI wraps in ```json fences for ReactMarkdown
 *   - TUI wraps as indented plain text
 */
export function transformInlineJson(
  text: string,
  wrap: (prefix: string, formatted: string) => string,
): string {
  const lines = text.split("\n");
  let inCodeBlock = false;
  const result: string[] = [];

  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      inCodeBlock = !inCodeBlock;
      result.push(line);
      continue;
    }

    if (inCodeBlock) {
      result.push(line);
      continue;
    }

    const trimmed = line.trim();
    if (!trimmed.includes("{") && !trimmed.includes("[")) {
      result.push(line);
      continue;
    }

    let transformed = false;
    for (let i = 0; i < trimmed.length; i++) {
      const ch = trimmed[i];
      if (ch !== "{" && ch !== "[") continue;
      const candidate = trimmed.slice(i);
      if (candidate.length < MIN_JSON_TRANSFORM_LENGTH) break;
      try {
        const parsed = JSON.parse(candidate);
        if (
          parsed !== null &&
          typeof parsed === "object" &&
          (Array.isArray(parsed) ? parsed.length > 0 : Object.keys(parsed).length > 0)
        ) {
          const prefix = trimmed.slice(0, i).trimEnd();
          const formatted = JSON.stringify(parsed, null, 2);
          result.push(wrap(prefix, formatted));
          transformed = true;
          break;
        }
      } catch {
        // not valid JSON from this position — try next {/[ character
      }
    }

    if (!transformed) {
      result.push(line);
    }
  }

  return result.join("\n");
}

/** Relative time: "3m ago", "2h ago", "5d ago" */
export function timeAgo(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(ms / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}
