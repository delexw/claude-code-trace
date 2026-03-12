import { describe, it, expect } from "vitest";
import type { DisplayMessage } from "../types";
import {
  shortModel,
  formatTokens,
  estimateCost,
  formatCost,
  formatDuration,
  shortPath,
  shortMode,
  contextPercent,
  formatExactTime,
  groupByDate,
  truncate,
  firstLine,
  formatJson,
  projectKey,
  projectDisplayName,
} from "./format";

// ---------------------------------------------------------------------------
// shortModel
// ---------------------------------------------------------------------------
describe("shortModel", () => {
  it("strips claude- prefix and formats major.minor version", () => {
    expect(shortModel("claude-opus-4-6")).toBe("opus4.6");
    expect(shortModel("claude-sonnet-4-6")).toBe("sonnet4.6");
  });

  it("drops patch/build metadata after minor version", () => {
    expect(shortModel("claude-haiku-4-5-20251001")).toBe("haiku4.5");
  });

  it("returns family name if no version dash present", () => {
    expect(shortModel("opus")).toBe("opus");
  });

  it("handles model without claude- prefix", () => {
    expect(shortModel("sonnet-4-6")).toBe("sonnet4.6");
  });

  it("handles single version number", () => {
    expect(shortModel("claude-opus-4")).toBe("opus4");
  });

  it("handles empty string", () => {
    expect(shortModel("")).toBe("");
  });

  it("handles claude- prefix only", () => {
    expect(shortModel("claude-")).toBe("");
  });
});

// ---------------------------------------------------------------------------
// formatTokens
// ---------------------------------------------------------------------------
describe("formatTokens", () => {
  it("returns raw number below 1000", () => {
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(1)).toBe("1");
    expect(formatTokens(500)).toBe("500");
    expect(formatTokens(999)).toBe("999");
  });

  it("formats thousands with k suffix", () => {
    expect(formatTokens(1000)).toBe("1.0k");
    expect(formatTokens(1234)).toBe("1.2k");
    expect(formatTokens(123456)).toBe("123.5k");
    expect(formatTokens(999999)).toBe("1000.0k");
  });

  it("formats millions with M suffix", () => {
    expect(formatTokens(1_000_000)).toBe("1.0M");
    expect(formatTokens(1_234_567)).toBe("1.2M");
    expect(formatTokens(10_500_000)).toBe("10.5M");
  });

  it("handles boundary at exactly 1000", () => {
    expect(formatTokens(1000)).toBe("1.0k");
  });

  it("handles boundary at exactly 1000000", () => {
    expect(formatTokens(1_000_000)).toBe("1.0M");
  });
});

// ---------------------------------------------------------------------------
// estimateCost
// ---------------------------------------------------------------------------
describe("estimateCost", () => {
  it("calculates cost for opus model", () => {
    // opus: input=5, output=25, cacheRead=0.5, cacheWrite=6.25 per million
    const cost = estimateCost(1_000_000, 0, 0, 0, "claude-opus-4-6");
    expect(cost).toBeCloseTo(5.0);
  });

  it("calculates cost for sonnet model", () => {
    // sonnet: input=3, output=15
    const cost = estimateCost(0, 1_000_000, 0, 0, "claude-sonnet-4-6");
    expect(cost).toBeCloseTo(15.0);
  });

  it("calculates cost for haiku model", () => {
    // haiku: input=1, output=5, cacheRead=0.1, cacheWrite=1.25
    const cost = estimateCost(0, 0, 1_000_000, 0, "haiku");
    expect(cost).toBeCloseTo(0.1);
  });

  it("includes all token types in calculation", () => {
    // opus pricing
    const cost = estimateCost(100_000, 50_000, 200_000, 10_000, "opus");
    const expected = (100_000 * 5 + 50_000 * 25 + 200_000 * 0.5 + 10_000 * 6.25) / 1_000_000;
    expect(cost).toBeCloseTo(expected);
  });

  it("defaults to sonnet pricing for unknown model", () => {
    const cost = estimateCost(1_000_000, 0, 0, 0, "unknown-model-xyz");
    expect(cost).toBeCloseTo(3.0); // sonnet input rate
  });

  it("returns 0 when all tokens are 0", () => {
    expect(estimateCost(0, 0, 0, 0, "opus")).toBe(0);
  });

  it("handles cache write cost for opus", () => {
    const cost = estimateCost(0, 0, 0, 1_000_000, "opus");
    expect(cost).toBeCloseTo(6.25);
  });
});

// ---------------------------------------------------------------------------
// formatCost
// ---------------------------------------------------------------------------
describe("formatCost", () => {
  it("always formats with 2 decimal places", () => {
    expect(formatCost(1.23)).toBe("$1.23");
    expect(formatCost(1.0)).toBe("$1.00");
    expect(formatCost(12.5)).toBe("$12.50");
    expect(formatCost(100)).toBe("$100.00");
    expect(formatCost(0.05)).toBe("$0.05");
    expect(formatCost(0.01)).toBe("$0.01");
    expect(formatCost(0.999)).toBe("$1.00");
    expect(formatCost(0.005)).toBe("$0.01");
    expect(formatCost(0.001)).toBe("$0.00");
    expect(formatCost(0)).toBe("$0.00");
  });

  it("rounds correctly", () => {
    expect(formatCost(1.235)).toBe("$1.24");
    expect(formatCost(1.234)).toBe("$1.23");
    expect(formatCost(0.0099)).toBe("$0.01");
    expect(formatCost(0.0049)).toBe("$0.00");
  });
});

// ---------------------------------------------------------------------------
// formatDuration
// ---------------------------------------------------------------------------
describe("formatDuration", () => {
  it("formats sub-10-second durations with one decimal", () => {
    expect(formatDuration(500)).toBe("0.5s");
    expect(formatDuration(0)).toBe("0.0s");
    expect(formatDuration(1234)).toBe("1.2s");
    expect(formatDuration(9999)).toBe("10.0s");
  });

  it("formats 10-59 second durations as whole seconds", () => {
    expect(formatDuration(10_000)).toBe("10s");
    expect(formatDuration(15_000)).toBe("15s");
    expect(formatDuration(59_999)).toBe("60s");
  });

  it("formats >= 60 seconds as minutes and seconds", () => {
    expect(formatDuration(60_000)).toBe("1m 0s");
    expect(formatDuration(90_000)).toBe("1m 30s");
    expect(formatDuration(125_000)).toBe("2m 5s");
    expect(formatDuration(3_600_000)).toBe("60m 0s");
  });

  it("handles exact minute boundaries", () => {
    expect(formatDuration(120_000)).toBe("2m 0s");
  });
});

// ---------------------------------------------------------------------------
// shortPath
// ---------------------------------------------------------------------------
describe("shortPath", () => {
  it("extracts last path segment", () => {
    expect(shortPath("/Users/foo/project")).toBe("project");
    expect(shortPath("/home/user/my-app")).toBe("my-app");
  });

  it("returns empty for empty string", () => {
    expect(shortPath("")).toBe("");
  });

  it("handles single segment path", () => {
    expect(shortPath("/project")).toBe("project");
  });

  it("handles trailing slash", () => {
    expect(shortPath("/Users/foo/project/")).toBe("project");
  });

  it("handles root path", () => {
    // split("/").filter(Boolean) on "/" gives [], so falls back to cwd
    expect(shortPath("/")).toBe("/");
  });

  it("ignores optional gitBranch parameter", () => {
    expect(shortPath("/Users/foo/project", "main")).toBe("project");
  });
});

// ---------------------------------------------------------------------------
// shortMode
// ---------------------------------------------------------------------------
describe("shortMode", () => {
  it("maps known modes to short labels", () => {
    expect(shortMode("default")).toBe("default");
    expect(shortMode("acceptEdits")).toBe("auto-edit");
    expect(shortMode("bypassPermissions")).toBe("yolo");
    expect(shortMode("plan")).toBe("plan");
  });

  it("returns unknown mode as-is", () => {
    expect(shortMode("customMode")).toBe("customMode");
    expect(shortMode("")).toBe("");
  });
});

// ---------------------------------------------------------------------------
// contextPercent
// ---------------------------------------------------------------------------
function makeContextMsg(role: DisplayMessage["role"], context_tokens: number): DisplayMessage {
  return {
    role,
    model: "",
    content: "",
    timestamp: "",
    thinking_count: 0,
    tool_call_count: 0,
    output_count: 0,
    tokens_raw: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    context_tokens,
    duration_ms: 0,
    items: [],
    last_output: null,
    is_error: false,
    teammate_spawns: 0,
    teammate_messages: 0,
    subagent_label: "",
  };
}

describe("contextPercent", () => {
  it("returns -1 for empty messages", () => {
    expect(contextPercent([])).toBe(-1);
  });

  it("returns -1 when no claude messages have context_tokens", () => {
    const msgs = [makeContextMsg("user", 0), makeContextMsg("claude", 0)];
    expect(contextPercent(msgs)).toBe(-1);
  });

  it("finds the last claude message with context_tokens", () => {
    const msgs = [
      makeContextMsg("claude", 50_000),
      makeContextMsg("user", 0),
      makeContextMsg("claude", 100_000),
    ];
    // 100_000 / 200_000 = 50%
    expect(contextPercent(msgs)).toBe(50);
  });

  it("skips non-claude messages", () => {
    const msgs = [
      makeContextMsg("claude", 60_000),
      makeContextMsg("user", 150_000), // user messages should be skipped
    ];
    expect(contextPercent(msgs)).toBe(30); // 60_000 / 200_000 = 30%
  });

  it("caps at 100%", () => {
    const msgs = [makeContextMsg("claude", 250_000)];
    expect(contextPercent(msgs)).toBe(100);
  });

  it("floors the percentage", () => {
    // 1_000 / 200_000 = 0.5% -> floors to 0
    const msgs = [makeContextMsg("claude", 1_000)];
    expect(contextPercent(msgs)).toBe(0);
  });

  it("returns correct value for full context", () => {
    const msgs = [makeContextMsg("claude", 200_000)];
    expect(contextPercent(msgs)).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// formatExactTime
// ---------------------------------------------------------------------------
describe("formatExactTime", () => {
  it("formats ISO timestamp to yyyy-mm-dd hh:mm:ss", () => {
    // Use a known UTC time and check the format structure
    const result = formatExactTime("2024-06-15T10:30:45Z");
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  });

  it("returns empty string for empty input", () => {
    expect(formatExactTime("")).toBe("");
  });

  it("returns empty string for invalid date", () => {
    expect(formatExactTime("not-a-date")).toBe("");
  });

  it("pads single-digit components with zeros", () => {
    // January 5 at 3:04:07 UTC
    const result = formatExactTime("2024-01-05T03:04:07Z");
    expect(result).toMatch(/^\d{4}-01-05 \d{2}:\d{2}:\d{2}$/);
  });
});

// ---------------------------------------------------------------------------
// groupByDate
// ---------------------------------------------------------------------------
describe("groupByDate", () => {
  it("returns empty array for empty input", () => {
    expect(groupByDate([])).toEqual([]);
  });

  it("groups items into Today", () => {
    const now = new Date();
    const items = [{ mod_time: now.toISOString(), id: 1 }];
    const result = groupByDate(items);
    expect(result).toHaveLength(1);
    expect(result[0].category).toBe("Today");
    expect(result[0].items).toHaveLength(1);
  });

  it("groups items into Yesterday", () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    yesterday.setHours(12, 0, 0, 0); // midday yesterday
    const items = [{ mod_time: yesterday.toISOString(), id: 1 }];
    const result = groupByDate(items);
    expect(result.some((g) => g.category === "Yesterday")).toBe(true);
  });

  it("sorts items within group by mod_time descending", () => {
    const now = new Date();
    const earlier = new Date(now.getTime() - 3600000); // 1 hour ago
    const items = [
      { mod_time: earlier.toISOString(), id: "earlier" },
      { mod_time: now.toISOString(), id: "now" },
    ];
    const result = groupByDate(items);
    expect(result[0].items[0].id).toBe("now");
    expect(result[0].items[1].id).toBe("earlier");
  });

  it("places old items in Older category", () => {
    const old = new Date("2020-01-01T00:00:00Z");
    const items = [{ mod_time: old.toISOString(), id: 1 }];
    const result = groupByDate(items);
    expect(result[0].category).toBe("Older");
  });

  it("preserves category order: Today, Yesterday, This Week, This Month, Older", () => {
    const now = new Date();
    const todayItem = { mod_time: now.toISOString(), id: "today" };
    const olderItem = { mod_time: "2020-01-01T00:00:00Z", id: "older" };

    const result = groupByDate([olderItem, todayItem]);
    expect(result[0].category).toBe("Today");
    expect(result[1].category).toBe("Older");
  });

  it("omits empty categories from output", () => {
    const now = new Date();
    const items = [{ mod_time: now.toISOString(), id: 1 }];
    const result = groupByDate(items);
    // Should only have Today, no empty categories
    expect(result.every((g) => g.items.length > 0)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// truncate
// ---------------------------------------------------------------------------
describe("truncate", () => {
  it("returns text unchanged when within limit", () => {
    expect(truncate("hello", 10)).toBe("hello");
    expect(truncate("hello", 5)).toBe("hello");
  });

  it("truncates with ellipsis when over limit", () => {
    const result = truncate("hello world", 6);
    expect(result).toBe("hello\u2026");
    expect(result.length).toBe(6);
  });

  it("handles empty string", () => {
    expect(truncate("", 5)).toBe("");
  });

  it("handles maxLen of 1", () => {
    expect(truncate("hello", 1)).toBe("\u2026");
  });

  it("handles exact length match", () => {
    expect(truncate("abc", 3)).toBe("abc");
  });

  it("handles maxLen of 0", () => {
    // slice(0, -1) gives "hell", plus ellipsis
    expect(truncate("hello", 0)).toBe("hell\u2026");
  });
});

// ---------------------------------------------------------------------------
// firstLine
// ---------------------------------------------------------------------------
describe("firstLine", () => {
  it("returns full text when no newline", () => {
    expect(firstLine("hello world")).toBe("hello world");
  });

  it("returns first line when newlines present", () => {
    expect(firstLine("first\nsecond\nthird")).toBe("first");
  });

  it("returns empty string for text starting with newline", () => {
    expect(firstLine("\nsecond")).toBe("");
  });

  it("handles empty string", () => {
    expect(firstLine("")).toBe("");
  });

  it("handles single newline", () => {
    expect(firstLine("\n")).toBe("");
  });
});

// ---------------------------------------------------------------------------
// formatJson
// ---------------------------------------------------------------------------
describe("formatJson", () => {
  it("pretty-prints valid JSON", () => {
    expect(formatJson('{"a":1,"b":2}')).toBe('{\n  "a": 1,\n  "b": 2\n}');
  });

  it("returns original string for invalid JSON", () => {
    expect(formatJson("not json")).toBe("not json");
    expect(formatJson("{invalid}")).toBe("{invalid}");
  });

  it("handles JSON arrays", () => {
    expect(formatJson("[1,2,3]")).toBe("[\n  1,\n  2,\n  3\n]");
  });

  it("handles empty object", () => {
    expect(formatJson("{}")).toBe("{}");
  });

  it("handles empty array", () => {
    expect(formatJson("[]")).toBe("[]");
  });

  it("handles nested JSON", () => {
    const input = '{"a":{"b":1}}';
    const expected = '{\n  "a": {\n    "b": 1\n  }\n}';
    expect(formatJson(input)).toBe(expected);
  });

  it("handles empty string", () => {
    expect(formatJson("")).toBe("");
  });
});

// ---------------------------------------------------------------------------
// projectKey
// ---------------------------------------------------------------------------
describe("projectKey", () => {
  it("extracts project key from typical session path", () => {
    expect(projectKey("/Users/x/.claude/projects/foo-bar/session.jsonl")).toBe("foo-bar");
  });

  it("extracts key from deeply nested path", () => {
    expect(projectKey("/home/user/.claude/projects/my-project/sub/deep/file.jsonl")).toBe(
      "my-project",
    );
  });

  it('returns "unknown" when no projects segment found', () => {
    expect(projectKey("/Users/x/random/path")).toBe("unknown");
  });

  it('returns "unknown" for empty string', () => {
    expect(projectKey("")).toBe("unknown");
  });

  it("handles URL-encoded project keys", () => {
    expect(projectKey("/Users/x/.claude/projects/my%20project/session.jsonl")).toBe("my%20project");
  });

  it("handles project key with special characters", () => {
    expect(projectKey("/Users/x/.claude/projects/proj_123-abc/session.jsonl")).toBe("proj_123-abc");
  });
});

// ---------------------------------------------------------------------------
// projectDisplayName
// ---------------------------------------------------------------------------
describe("projectDisplayName", () => {
  it("decodes typical project key to last segment", () => {
    expect(projectDisplayName("-Users-yang-liu-Envato-others-my-project")).toBe("project");
  });

  it("decodes scheduler key", () => {
    expect(projectDisplayName("-Users-yang-liu--claude-scheduler")).toBe("scheduler");
  });

  it("decodes .claude key", () => {
    expect(projectDisplayName("-Users-yang-liu-.claude")).toBe(".claude");
  });

  it("returns key as fallback for empty decode", () => {
    expect(projectDisplayName("")).toBe("");
  });
});
