import type { DisplayMessage } from "../../types";
import type { CodexSession, CodexWindow } from "./types";

// Entirely fabricated examples. Never populate these from local chat logs.
export function makeSession(overrides: Partial<CodexSession> = {}): CodexSession {
  return {
    id: "sample-session",
    path: "/sample/.codex/sessions/rollout-sample.jsonl",
    title: "Repair the sample build",
    cwd: "/work/sample",
    model: "gpt-sample",
    modified_at: "2026-01-02T12:00:00Z",
    archived: false,
    source: "local",
    total_tokens: 30,
    cost_usd: 0.25,
    ...overrides,
  };
}

export function makeMessage(overrides: Partial<DisplayMessage> = {}): DisplayMessage {
  return {
    role: "claude",
    model: "gpt-sample",
    content: "The sample build now passes.",
    timestamp: "2026-01-02T12:00:00Z",
    thinking_count: 0,
    tool_call_count: 0,
    output_count: 1,
    tokens_raw: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    context_tokens: 0,
    duration_ms: 0,
    items: [],
    last_output: null,
    is_error: false,
    teammate_spawns: 0,
    teammate_messages: 0,
    subagent_label: "",
    ...overrides,
  };
}

export function makeWindow(overrides: Partial<CodexWindow> = {}): CodexWindow {
  return {
    messages: [makeMessage()],
    count: 1,
    start: 0,
    roles: ["claude"],
    ongoing: false,
    total_tokens: 30,
    input_tokens: 20,
    output_tokens: 10,
    cache_read_tokens: 0,
    revision: "sample-revision",
    cost_usd: 0.25,
    ...overrides,
  };
}
