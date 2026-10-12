import type {
  AntigravityMessage,
  AntigravityToolCall,
  DisplayItem,
  DisplayMessage,
} from "../types";

/**
 * How one Antigravity step type draws: the modifier class that colours the
 * row's left border, the role text, and the `message__role--*` modifier.
 *
 * The class and label come straight from the Antigravity vocabulary — a tool
 * call is purple and reads "MCP Tool", not a grey "System". `DisplayMessage.role`
 * cannot carry that (it has five values, none of them `tool`/`file`/`search`),
 * so the renderer takes these three as overrides and only uses `role` for the
 * icon and the shared `StatsBar`.
 */
export interface AntigravityRoleRender {
  className: string;
  label: string;
  modifier: string;
}

const ROLE_RENDER: Record<AntigravityMessage["role"], AntigravityRoleRender> = {
  user: { className: "message--user", label: "User", modifier: "user" },
  assistant: { className: "message--claude", label: "Assistant", modifier: "claude" },
  tool: { className: "message--tool", label: "MCP Tool", modifier: "tool" },
  file: { className: "message--file", label: "View File", modifier: "file" },
  search: { className: "message--search", label: "Grep Search", modifier: "search" },
  error: { className: "message--system-error", label: "Error", modifier: "system-error" },
  checkpoint: { className: "message--compact", label: "Checkpoint", modifier: "compact" },
  context: { className: "message--context", label: "Context", modifier: "context" },
  system: { className: "message--system", label: "System", modifier: "system" },
};

/**
 * Map one normalized brain step onto the shared `DisplayMessage` so the Claude
 * renderer can draw it unchanged.
 *
 * Only values the transcript actually carries are filled: tool and thinking
 * counts come from the row's own `tool_calls` and `thinking` keys, duration
 * from the gap to the next row. Token counts stay 0 because a brain transcript
 * records none — and `StatsBar` hides a zero rather than inventing a number.
 */
export function toDisplayMessage(message: AntigravityMessage): DisplayMessage {
  const isAssistant = message.role === "assistant";
  return {
    // `DisplayMessage.role` only knows five values, so the four step types
    // with no counterpart (tool / file / search / context) collapse to
    // `system`; their colour and label ride along as renderer overrides.
    role:
      message.role === "user"
        ? "user"
        : isAssistant
          ? "claude"
          : message.role === "checkpoint"
            ? "compact"
            : "system",
    model: "",
    content: body(message),
    timestamp: message.created_at ?? "",
    thinking_count: message.thinking_text ? 1 : 0,
    tool_call_count: message.tool_calls.length,
    output_count: isAssistant ? 1 : 0,
    // A brain transcript carries no token accounting at all.
    tokens_raw: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    context_tokens: 0,
    duration_ms: message.duration_ms,
    items: [
      ...(message.thinking_text ? [thinkingItem(message)] : []),
      ...message.tool_calls.map((call, index) => toolItem(message, call, index)),
    ],
    last_output: null,
    is_error: message.role === "error",
    teammate_spawns: 0,
    teammate_messages: 0,
    subagent_label: "",
  };
}

/**
 * What the row actually says, when `content` is blank.
 *
 * 41% of real brain rows carry no `content` at all — 8,829 of the 11,059
 * `PLANNER_RESPONSE` rows among them, because a planner turn that only thought
 * or only called a tool writes its payload into `thinking` or `tool_calls`
 * instead. Rendering `content` alone showed those as blank cards, which read as
 * a broken parse rather than as a step that simply said nothing in prose.
 *
 * Every line below is text the transcript really contains — nothing is
 * invented to fill the gap. A row that has none of the four stays empty, and
 * an empty body is the honest result for it.
 */
function body(message: AntigravityMessage): string {
  if (message.content.trim()) return message.content;
  const parts: string[] = [];
  if (message.thinking_text) parts.push(message.thinking_text);
  for (const call of message.tool_calls) {
    parts.push(call.summary ? `${call.name} — ${call.summary}` : call.name);
  }
  if (message.error_text) parts.push(message.error_text);
  return parts.join("\n\n");
}

/** The row's `thinking` block as a `Thinking` item, so the detail panel shows
 *  it the way it shows a Claude thinking block. */
function thinkingItem(message: AntigravityMessage): DisplayItem {
  return {
    ...EMPTY_ITEM,
    id: `ag-${message.step_index}-thinking`,
    item_type: "Thinking",
    text: message.thinking_text ?? "",
  };
}

/** A `DisplayItem` with every field at its empty value. `DisplayItem` has no
 *  optional fields, so a brain row fills only the handful it has data for and
 *  inherits the rest — the alternative is mirroring 31 fields per item. */
const EMPTY_ITEM: DisplayItem = {
  id: "",
  item_type: "Thinking",
  text: "",
  tool_name: "",
  tool_summary: "",
  tool_category: "",
  tool_input: "",
  tool_result: "",
  tool_error: false,
  duration_ms: 0,
  token_count: 0,
  subagent_type: "",
  subagent_desc: "",
  subagent_prompt: "",
  team_member_name: "",
  teammate_id: "",
  team_color: "",
  subagent_ongoing: false,
  agent_id: "",
  subagent_messages: [],
  hook_event: "",
  hook_name: "",
  hook_command: "",
  hook_metadata: "",
  tool_result_json: "",
  is_orphan: false,
  is_deferred: false,
  hook_source_agent_name: "",
  hook_requesting_agent_uuid: "",
  advisor_model: "",
  tool_denial_kind: "",
};

/** One `DisplayItem` per tool call, so the detail panel and the `Detail` button
 *  have something real to show — the same shape a Claude tool call gets. */
function toolItem(
  message: AntigravityMessage,
  call: AntigravityToolCall,
  index: number,
): DisplayItem {
  return {
    ...EMPTY_ITEM,
    id: `ag-${message.step_index}-${index}`,
    item_type: "ToolCall",
    text: call.summary,
    tool_name: call.name,
    tool_summary: call.summary,
    // The step's real payload. For a content-less row this is the only place
    // it exists — leaving it blank rendered an empty item body.
    tool_input: call.input,
    // The row's `content` is the tool's own output for an MCP_TOOL row, and the
    // step's text for a planner response — either way it is what the step says.
    tool_result: message.content,
  };
}

export function toDisplayMessages(messages: AntigravityMessage[]): DisplayMessage[] {
  return messages.map(toDisplayMessage);
}

/**
 * Whether a step carries anything a reader can see: text, a thought, a tool
 * call, or an error.
 *
 * 1,847 of 25,064 real brain rows carry none of the four — an `EPHEMERAL_MESSAGE`
 * with no content, a `PLANNER_RESPONSE` that only advanced a cursor. They are
 * real steps, but as cards they are a header, a timestamp and silence, and they
 * make a conversation look broken. The viewer drops them; `toDisplayMessage`
 * still handles them for callers that want the raw row.
 */
export function hasPayload(message: AntigravityMessage): boolean {
  return Boolean(
    (typeof message.content === "string" && message.content.trim()) ||
    message.thinking_text ||
    message.error_text ||
    message.tool_calls.length > 0,
  );
}

/** The draw vocabulary for a step type, for the renderer's overrides. */
export function roleRender(role: AntigravityMessage["role"]): AntigravityRoleRender {
  return ROLE_RENDER[role];
}
