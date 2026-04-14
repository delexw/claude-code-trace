import { describe, it, expect, vi } from "vitest";
import { createRef } from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { MessageList } from "./MessageList";
import type { DisplayMessage } from "../types";
import type { ViewActions } from "../hooks/useViewActions";

function makeMessage(overrides: Partial<DisplayMessage> = {}): DisplayMessage {
  return {
    role: "user",
    model: "",
    content: "Hello from user",
    timestamp: "2025-01-01T12:00:00Z",
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

function defaultProps(overrides: Partial<Parameters<typeof MessageList>[0]> = {}) {
  return {
    messages: [] as DisplayMessage[],
    selectedIndex: -1,
    expandedSet: new Set<number>(),
    ongoing: false,
    onSelect: vi.fn(),
    onToggle: vi.fn(),
    onOpenDetail: vi.fn(),
    viewActionsRef: createRef() as React.MutableRefObject<ViewActions>,
    onExpandAll: vi.fn(),
    onCollapseAll: vi.fn(),
    ...overrides,
  };
}

describe("MessageList", () => {
  it("shows 'No messages loaded' when empty", () => {
    render(<MessageList {...defaultProps()} />);
    expect(screen.getByText("No messages loaded")).toBeInTheDocument();
  });

  it("renders messages in chronological order (oldest first)", () => {
    const messages = [
      makeMessage({ content: "First message", role: "user" }),
      makeMessage({ content: "Second message", role: "claude", model: "claude-sonnet-4-20250514" }),
    ];
    const { container } = render(<MessageList {...defaultProps({ messages })} />);
    const messageEls = container.querySelectorAll(".message");
    // First message (index 0) should appear first in the DOM
    expect(messageEls[0]).toHaveTextContent(/First message/);
    expect(messageEls[1]).toHaveTextContent(/Second message/);
  });

  it("shows compact separator for compact role", () => {
    const messages = [makeMessage({ role: "compact", content: "--- separator ---" })];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText("--- separator ---")).toBeInTheDocument();
    const { container } = render(<MessageList {...defaultProps({ messages })} />);
    expect(container.querySelector(".compact-separator")).toBeInTheDocument();
  });

  it("shows correct role labels for user, claude, system", () => {
    const messages = [
      makeMessage({ role: "user", content: "user msg" }),
      makeMessage({ role: "claude", content: "claude msg", model: "claude-sonnet-4-20250514" }),
      makeMessage({ role: "system", content: "system msg" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText("User")).toBeInTheDocument();
    expect(screen.getByText("Claude", { selector: ".message__role" })).toBeInTheDocument();
    expect(screen.getByText("System")).toBeInTheDocument();
  });

  it("shows model color for claude messages", () => {
    const messages = [
      makeMessage({ role: "claude", content: "Response", model: "claude-sonnet-4-20250514" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    const modelEl = screen.getByText("sonnet4.20250514");
    expect(modelEl).toBeInTheDocument();
    expect(modelEl).toHaveStyle({ color: "#5fafff" }); // modelSonnet color
  });

  it("clicking selects message; clicking selected toggles expand", () => {
    const onSelect = vi.fn();
    const onToggle = vi.fn();
    const messages = [makeMessage({ content: "Click me" })];

    // First click: message is not selected, should call onSelect
    const { rerender } = render(
      <MessageList {...defaultProps({ messages, selectedIndex: -1, onSelect, onToggle })} />,
    );
    fireEvent.click(screen.getByText(/Click me/).closest(".message")!);
    expect(onSelect).toHaveBeenCalledWith(0);

    // Second click: message is already selected, should call onToggle
    rerender(<MessageList {...defaultProps({ messages, selectedIndex: 0, onSelect, onToggle })} />);
    fireEvent.click(screen.getByText(/Click me/).closest(".message")!);
    expect(onToggle).toHaveBeenCalledWith(0);
  });

  it("double-click opens detail", () => {
    const onOpenDetail = vi.fn();
    const messages = [makeMessage({ content: "Double click me" })];
    render(<MessageList {...defaultProps({ messages, onOpenDetail })} />);
    fireEvent.doubleClick(screen.getByText(/Double click me/).closest(".message")!);
    expect(onOpenDetail).toHaveBeenCalledWith(0);
  });

  it("shows stats when tokens present", () => {
    const messages = [
      makeMessage({ role: "claude", tokens_raw: 5000, model: "claude-sonnet-4-20250514" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText(/5\.0k tok/)).toBeInTheDocument();
  });

  it("shows stats for tools", () => {
    const messages = [
      makeMessage({ role: "claude", tool_call_count: 3, model: "claude-sonnet-4-20250514" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText(/3 tools/)).toBeInTheDocument();
  });

  it("shows stats for thinking", () => {
    const messages = [
      makeMessage({ role: "claude", thinking_count: 2, model: "claude-sonnet-4-20250514" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText(/2 think/)).toBeInTheDocument();
  });

  it("shows stats for duration", () => {
    const messages = [
      makeMessage({ role: "claude", duration_ms: 5000, model: "claude-sonnet-4-20250514" }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText("5.0s")).toBeInTheDocument();
  });

  it("shows stats for agents (subagents)", () => {
    const messages = [
      makeMessage({
        role: "claude",
        model: "claude-sonnet-4-20250514",
        items: [
          {
            id: "a1",
            item_type: "Subagent",
            text: "",
            tool_name: "",
            tool_summary: "",
            tool_category: "",
            tool_input: "",
            tool_result: "",
            tool_error: false,
            duration_ms: 0,
            token_count: 0,
            subagent_type: "task",
            subagent_desc: "agent",
            subagent_prompt: "",
            team_member_name: "",
            teammate_id: "",
            team_color: "",
            subagent_ongoing: false,
            agent_id: "a1",
            subagent_messages: [],
            hook_event: "",
            hook_name: "",
            hook_command: "",
            hook_metadata: "",
            tool_result_json: "",
            is_orphan: false,
          },
        ],
      }),
    ];
    render(<MessageList {...defaultProps({ messages })} />);
    expect(screen.getByText(/1 agent/)).toBeInTheDocument();
  });

  it("shows ongoing dots for last message when ongoing", () => {
    const messages = [
      makeMessage({ role: "user", content: "First" }),
      makeMessage({ role: "claude", content: "Second", model: "claude-sonnet-4-20250514" }),
    ];
    const { container } = render(<MessageList {...defaultProps({ messages, ongoing: true })} />);
    // The ongoing dots should be on the last message (the one actively being processed)
    const dots = container.querySelectorAll(".ongoing-dots");
    expect(dots.length).toBe(1);
  });

  it("does not show ongoing dots when ongoing=false", () => {
    const messages = [makeMessage({ content: "No spinner" })];
    const { container } = render(<MessageList {...defaultProps({ messages, ongoing: false })} />);
    expect(container.querySelector(".ongoing-dots")).not.toBeInTheDocument();
  });
});
