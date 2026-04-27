import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DisplayItem, DisplayMessage } from "../types";
import type { ViewActionsRef } from "../hooks/useViewActions";
import { MessageDetail } from "./MessageDetail";

vi.mock("react-syntax-highlighter", () => ({
  Prism: ({ children }: { children: React.ReactNode }) => <pre>{children}</pre>,
}));

vi.mock("react-syntax-highlighter/dist/esm/styles/prism", () => ({ oneDark: {} }));

Element.prototype.scrollTo = vi.fn();

function makeItem(overrides: Partial<DisplayItem> = {}): DisplayItem {
  return {
    id: "item-1",
    item_type: "ToolCall",
    text: "",
    tool_name: "Read",
    tool_summary: "file.ts",
    tool_category: "Read",
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
    ...overrides,
  };
}

function makeMessage(overrides: Partial<DisplayMessage> = {}): DisplayMessage {
  return {
    role: "claude",
    model: "claude-sonnet-4",
    content: "",
    timestamp: "2026-04-27T00:00:00Z",
    thinking_count: 0,
    tool_call_count: 0,
    output_count: 0,
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

function makeViewActionsRef(): ViewActionsRef {
  return { current: {} };
}

function makeRootMessage(nestedItems: DisplayItem[]): DisplayMessage {
  const subagentMessage = makeMessage({
    role: "claude",
    timestamp: "2026-04-27T00:00:01Z",
    content: "worker output",
    items: nestedItems,
  });

  return makeMessage({
    items: [
      makeItem({
        id: "subagent-1",
        item_type: "Subagent",
        tool_name: "",
        tool_summary: "",
        subagent_type: "Explore",
        subagent_desc: "Search code",
        agent_id: "agent-1",
        subagent_messages: [subagentMessage],
      }),
    ],
  });
}

describe("MessageDetail", () => {
  it("refreshes an open subagent panel when an existing subagent message gains a tool item", () => {
    const firstTool = makeItem({ id: "nested-1", tool_name: "Read", tool_summary: "first.ts" });
    const secondTool = makeItem({ id: "nested-2", tool_name: "Bash", tool_summary: "npm test" });

    const { rerender } = render(
      <MessageDetail
        message={makeRootMessage([firstTool])}
        onBack={vi.fn()}
        viewActionsRef={makeViewActionsRef()}
      />,
    );

    fireEvent.click(screen.getByText("Explore"));
    fireEvent.doubleClick(screen.getByText("worker output"));

    expect(screen.getByText("Read")).toBeInTheDocument();
    expect(screen.queryByText("Bash")).not.toBeInTheDocument();

    rerender(
      <MessageDetail
        message={makeRootMessage([firstTool, secondTool])}
        onBack={vi.fn()}
        viewActionsRef={makeViewActionsRef()}
      />,
    );

    expect(screen.getByText("Bash")).toBeInTheDocument();
  });
});
