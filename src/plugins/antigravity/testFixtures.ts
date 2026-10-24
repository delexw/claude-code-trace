import type { AntigravityBrainFolder, AntigravityMessage } from "./types";

export function makeStep(overrides: Partial<AntigravityMessage> = {}): AntigravityMessage {
  return {
    step_index: 0,
    role: "assistant",
    content: "",
    created_at: "2026-01-02T01:00:00Z",
    tool_calls: [],
    thinking_text: null,
    error_text: null,
    duration_ms: 0,
    ...overrides,
  };
}

export function makeFolder(
  overrides: Partial<AntigravityBrainFolder> = {},
): AntigravityBrainFolder {
  return {
    uuid: "11111111-2222-3333-4444-555555555555",
    path: "/home/user/.gemini/antigravity-ide/brain/11111111-2222-3333-4444-555555555555",
    mod_time_ms: Date.parse("2026-01-02T01:00:00Z"),
    title: "Fix the flaky build",
    flavor: "antigravity-ide",
    has_transcript: true,
    ...overrides,
  };
}
