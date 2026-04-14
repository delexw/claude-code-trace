import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DetailItem, getItemName, getItemSummary } from "./DetailItem";
import type { DisplayItem } from "../types";

function makeItem(overrides: Partial<DisplayItem> = {}): DisplayItem {
  return {
    id: "ToolCall-0",
    item_type: "ToolCall",
    text: "",
    tool_name: "Read",
    tool_summary: "file.ts",
    tool_category: "Read",
    tool_input: '{"path":"file.ts"}',
    tool_result: "file contents",
    tool_error: false,
    duration_ms: 1500,
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

describe("getItemName", () => {
  it("returns 'Thinking' for Thinking items", () => {
    expect(getItemName(makeItem({ item_type: "Thinking" }))).toBe("Thinking");
  });

  it("returns 'Output' for Output items", () => {
    expect(getItemName(makeItem({ item_type: "Output" }))).toBe("Output");
  });

  it("returns tool_name for ToolCall items", () => {
    expect(getItemName(makeItem({ item_type: "ToolCall", tool_name: "Bash" }))).toBe("Bash");
  });

  it("returns 'Tool' when tool_name is empty", () => {
    expect(getItemName(makeItem({ item_type: "ToolCall", tool_name: "" }))).toBe("Tool");
  });

  it("returns subagent_type for Subagent items", () => {
    expect(getItemName(makeItem({ item_type: "Subagent", subagent_type: "Explore" }))).toBe(
      "Explore",
    );
  });

  it("returns 'Subagent' when subagent_type is empty", () => {
    expect(getItemName(makeItem({ item_type: "Subagent", subagent_type: "" }))).toBe("Subagent");
  });

  it("returns team_member_name for TeammateMessage items", () => {
    expect(getItemName(makeItem({ item_type: "TeammateMessage", team_member_name: "Alice" }))).toBe(
      "Alice",
    );
  });

  it("returns hook_event for HookEvent items", () => {
    expect(getItemName(makeItem({ item_type: "HookEvent", hook_event: "PreToolUse" }))).toBe(
      "PreToolUse",
    );
  });
});

describe("getItemSummary", () => {
  it("returns tool_summary for ToolCall", () => {
    expect(getItemSummary(makeItem({ item_type: "ToolCall", tool_summary: "read file.ts" }))).toBe(
      "read file.ts",
    );
  });

  it("returns subagent_desc for Subagent", () => {
    expect(
      getItemSummary(makeItem({ item_type: "Subagent", subagent_desc: "search codebase" })),
    ).toBe("search codebase");
  });

  it("truncates Thinking text at 80 chars with ellipsis", () => {
    const longText = "a".repeat(100);
    const result = getItemSummary(makeItem({ item_type: "Thinking", text: longText }));
    expect(result).toHaveLength(81); // 80 chars + ellipsis
    expect(result).toMatch(/\u2026$/);
  });

  it("returns 'Content not recorded' for Thinking with no text", () => {
    expect(getItemSummary(makeItem({ item_type: "Thinking", text: "" }))).toBe(
      "Content not recorded",
    );
  });

  it("returns empty string for Output with no text", () => {
    expect(getItemSummary(makeItem({ item_type: "Output", text: "" }))).toBe("");
  });

  it("returns hook_name and command for HookEvent", () => {
    const result = getItemSummary(
      makeItem({ item_type: "HookEvent", hook_name: "format", hook_command: "prettier ." }),
    );
    expect(result).toBe("format: prettier .");
  });

  it("returns only hook_command when hook_name is empty", () => {
    const result = getItemSummary(
      makeItem({ item_type: "HookEvent", hook_name: "", hook_command: "npm test" }),
    );
    expect(result).toBe("npm test");
  });
});

describe("DetailItem", () => {
  it("renders item name and summary", () => {
    render(
      <DetailItem
        item={makeItem()}
        index={0}
        isSelected={false}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("Read")).toBeInTheDocument();
    expect(screen.getByText("file.ts")).toBeInTheDocument();
  });

  it("shows duration when present", () => {
    render(
      <DetailItem
        item={makeItem({ duration_ms: 5000 })}
        index={0}
        isSelected={false}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("5.0s")).toBeInTheDocument();
  });

  it("applies selected class when isSelected", () => {
    const { container } = render(
      <DetailItem
        item={makeItem()}
        index={0}
        isSelected={true}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(container.querySelector(".detail-item--selected")).toBeInTheDocument();
  });

  it("applies error class when tool_error is true", () => {
    const { container } = render(
      <DetailItem
        item={makeItem({ tool_error: true })}
        index={0}
        isSelected={false}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(container.querySelector(".detail-item--error")).toBeInTheDocument();
  });

  it("shows expanded body with tool input/result when expanded", () => {
    render(
      <DetailItem
        item={makeItem({
          tool_input: '{"path":"src/main.ts"}',
          tool_result: "file content here",
        })}
        index={0}
        isSelected={false}
        isExpanded={true}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("Input")).toBeInTheDocument();
    expect(screen.getByText("Result")).toBeInTheDocument();
    expect(screen.getByText("file content here")).toBeInTheDocument();
  });

  it("shows thinking body with italic text when expanded", () => {
    const { container } = render(
      <DetailItem
        item={makeItem({ item_type: "Thinking", text: "Let me think about this" })}
        index={0}
        isSelected={false}
        isExpanded={true}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    // Text appears in both summary and body
    expect(screen.getAllByText(/Let me think about this/).length).toBeGreaterThanOrEqual(1);
    expect(container.querySelector(".detail-item__text--thinking")).toBeInTheDocument();
  });

  it("shows orphan badge when is_orphan is true", () => {
    render(
      <DetailItem
        item={makeItem({ is_orphan: true })}
        index={0}
        isSelected={false}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("orphan")).toBeInTheDocument();
  });

  it("calls onToggle when header is clicked", () => {
    const onToggle = vi.fn();
    render(
      <DetailItem
        item={makeItem()}
        index={2}
        isSelected={false}
        isExpanded={false}
        onToggle={onToggle}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("Read").closest(".detail-item__header")!);
    expect(onToggle).toHaveBeenCalledWith(2, expect.objectContaining({ tool_name: "Read" }));
  });

  it("shows subagent prompt preview", () => {
    render(
      <DetailItem
        item={makeItem({
          item_type: "Subagent",
          subagent_type: "Explore",
          subagent_prompt: "Find all components",
        })}
        index={0}
        isSelected={false}
        isExpanded={false}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("Find all components")).toBeInTheDocument();
  });

  it("shows HookEvent body with event and command when expanded", () => {
    render(
      <DetailItem
        item={makeItem({
          item_type: "HookEvent",
          hook_event: "PreToolUse",
          hook_name: "format",
          hook_command: "prettier --write .",
        })}
        index={0}
        isSelected={false}
        isExpanded={true}
        onToggle={vi.fn()}
        onToggleExpand={vi.fn()}
        onSelect={vi.fn()}
      />,
    );
    expect(screen.getByText("Hook")).toBeInTheDocument();
    expect(screen.getByText("Command")).toBeInTheDocument();
    expect(screen.getByText("prettier --write .")).toBeInTheDocument();
  });
});
