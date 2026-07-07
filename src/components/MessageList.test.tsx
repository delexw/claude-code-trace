import { describe, it, expect, vi } from "vitest";
import { createRef, type ReactNode } from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { MessageList, selectionScrollTarget } from "./MessageList";
import type { DisplayMessage } from "../types";
import type { ViewActions } from "../hooks/useViewActions";

// react-virtuoso virtualises by measured height, which jsdom does not provide,
// so it would render nothing in tests. Mock it to render every row via
// itemContent(index, undefined, context) — these tests exercise MessageList's
// row rendering/callbacks, not the windowing.
// A hoisted spy for the Virtuoso imperative handle so tests can assert scrolls.
const virtuoso = vi.hoisted(() => ({ scrollToIndex: vi.fn() }));

vi.mock("react-virtuoso", async () => {
  const React = (await vi.importActual("react")) as typeof import("react");
  return {
    Virtuoso: React.forwardRef(function VirtuosoMock(
      {
        totalCount = 0,
        itemContent,
        context,
        className,
        isScrolling,
      }: {
        totalCount?: number;
        itemContent: (index: number, data: unknown, context: unknown) => ReactNode;
        context?: unknown;
        className?: string;
        isScrolling?: (scrolling: boolean) => void;
      },
      ref: React.Ref<{ scrollToIndex: (i: number) => void }>,
    ) {
      React.useImperativeHandle(ref, () => ({ scrollToIndex: virtuoso.scrollToIndex }));
      return (
        <div className={className}>
          {/* Test-only hooks to simulate Virtuoso reporting scroll start/stop. */}
          <button
            type="button"
            data-testid="simulate-scrolling"
            onClick={() => isScrolling?.(true)}
          />
          <button
            type="button"
            data-testid="simulate-scroll-stop"
            onClick={() => isScrolling?.(false)}
          />
          {Array.from({ length: totalCount }, (_, index) => (
            // oxlint-disable-next-line react/no-array-index-key
            <div key={index}>{itemContent(index, undefined, context)}</div>
          ))}
        </div>
      );
    }),
  };
});

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

/** Build MessageList props from a plain messages array (test convenience): the
 * component now takes count/getMessage/roles rather than the array itself. */
function defaultProps(
  overrides: { messages?: DisplayMessage[] } & Partial<Parameters<typeof MessageList>[0]> = {},
) {
  const { messages = [], ...rest } = overrides;
  return {
    count: messages.length,
    getMessage: (index: number): DisplayMessage | undefined => messages[index],
    roles: messages.map((m) => m.role),
    selectedIndex: -1,
    expandedSet: new Set<number>(),
    ongoing: false,
    onRangeChange: vi.fn(),
    onSelect: vi.fn(),
    onToggle: vi.fn(),
    onOpenDetail: vi.fn(),
    viewActionsRef: createRef() as React.MutableRefObject<ViewActions>,
    onExpandAll: vi.fn(),
    onCollapseAll: vi.fn(),
    ...rest,
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

  it("renders compact role as a message item with 'Compacted Message' label", () => {
    const messages = [makeMessage({ role: "compact", content: "--- summary ---" })];
    const { container } = render(<MessageList {...defaultProps({ messages })} />);
    expect(container.querySelector(".message")).toBeInTheDocument();
    expect(screen.getByText("Compacted Message")).toBeInTheDocument();
    expect(screen.getByText("--- summary ---")).toBeInTheDocument();
  });

  it("renders recap role as a message item with 'Session Recap' label", () => {
    const messages = [makeMessage({ role: "recap", content: "recap text" })];
    const { container } = render(<MessageList {...defaultProps({ messages })} />);
    expect(container.querySelector(".message")).toBeInTheDocument();
    expect(screen.getByText("Session Recap")).toBeInTheDocument();
    expect(screen.getByText("recap text")).toBeInTheDocument();
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
    const modelEl = screen.getByText("sonnet4");
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
            hook_source_agent_name: "",
            hook_requesting_agent_uuid: "",
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

  it("renders a role-aware placeholder skeleton for an unloaded index", () => {
    const { container } = render(
      <MessageList
        {...defaultProps({
          count: 2,
          roles: ["user", "claude"],
          getMessage: () => undefined,
        })}
      />,
    );
    const placeholders = container.querySelectorAll(".message--placeholder");
    expect(placeholders.length).toBe(2);
    expect(placeholders[0]).toHaveClass("message--user");
    expect(placeholders[1]).toHaveClass("message--claude");
    // Multi-line skeleton (header + content + short line), not a single sliver,
    // so the swap to real content is a small reflow rather than a large jump.
    expect(placeholders[0].querySelectorAll(".message__placeholder-line").length).toBe(3);
  });

  it("adds the scrolling class when Virtuoso reports isScrolling(true)", () => {
    const messages = [makeMessage({ content: "Hi" })];
    const { container, getByTestId } = render(<MessageList {...defaultProps({ messages })} />);

    expect(container.querySelector(".message-list")).not.toHaveClass("message-list--scrolling");
    fireEvent.click(getByTestId("simulate-scrolling"));
    expect(container.querySelector(".message-list")).toHaveClass("message-list--scrolling");
  });

  it("forces a hover recompute at the last cursor position once scrolling settles", async () => {
    vi.useFakeTimers();
    const messages = [makeMessage({ content: "Hi" })];
    const { getByTestId } = render(<MessageList {...defaultProps({ messages })} />);
    const dispatchSpy = vi.spyOn(document, "dispatchEvent");
    const rafSpy = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((cb) => window.setTimeout(() => cb(0), 0));

    fireEvent.mouseMove(window, { clientX: 42, clientY: 17 });
    fireEvent.click(getByTestId("simulate-scrolling"));
    fireEvent.click(getByTestId("simulate-scroll-stop"));
    vi.runAllTimers();

    const dispatched = dispatchSpy.mock.calls
      .map((call) => call[0])
      .find((event) => event.type === "mousemove") as MouseEvent | undefined;
    expect(dispatched).toBeDefined();
    expect(dispatched?.clientX).toBe(42);
    expect(dispatched?.clientY).toBe(17);

    rafSpy.mockRestore();
    dispatchSpy.mockRestore();
    vi.useRealTimers();
  });

  it("does not show ongoing dots when ongoing=false", () => {
    const messages = [makeMessage({ content: "No spinner" })];
    const { container } = render(<MessageList {...defaultProps({ messages, ongoing: false })} />);
    expect(container.querySelector(".ongoing-dots")).not.toBeInTheDocument();
  });
});

describe("selectionScrollTarget", () => {
  const range = { startIndex: 5, endIndex: 10 };

  it("returns null when nothing is selected", () => {
    expect(selectionScrollTarget(-1, range)).toBeNull();
  });

  it("returns null when the selection is within the rendered window", () => {
    expect(selectionScrollTarget(5, range)).toBeNull();
    expect(selectionScrollTarget(7, range)).toBeNull();
    expect(selectionScrollTarget(10, range)).toBeNull();
  });

  it("aligns to the top when the selection is above the window", () => {
    expect(selectionScrollTarget(2, range)).toEqual({ index: 2, align: "start" });
  });

  it("aligns to the end when the selection is below the window", () => {
    expect(selectionScrollTarget(15, range)).toEqual({ index: 15, align: "end" });
  });
});

describe("MessageList toolbar scroll registration", () => {
  it("registers scrollToTop/scrollToBottom that scroll virtuoso (reversed: top = newest = last index)", () => {
    virtuoso.scrollToIndex.mockClear();
    const messages = [makeMessage(), makeMessage(), makeMessage()]; // count === 3
    const viewActionsRef = createRef() as React.MutableRefObject<ViewActions>;
    render(<MessageList {...defaultProps({ messages, viewActionsRef })} />);

    expect(viewActionsRef.current.scrollToTop).toBeTypeOf("function");
    viewActionsRef.current.scrollToTop!();
    expect(virtuoso.scrollToIndex).toHaveBeenCalledWith(2); // newest at the visual top

    viewActionsRef.current.scrollToBottom!();
    expect(virtuoso.scrollToIndex).toHaveBeenCalledWith(0); // oldest at the visual bottom
  });
});
