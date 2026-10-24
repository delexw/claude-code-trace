import { describe, it, expect } from "vitest";
import { toDisplayMessage } from "./toDisplay";
import { roleStyle } from "./roleStyles";
import { makeStep } from "./testFixtures";

describe("toDisplayMessage", () => {
  it.each([
    ["user", "user"],
    ["assistant", "claude"],
    ["checkpoint", "compact"],
    ["tool", "system"],
    ["file", "system"],
    ["search", "system"],
    ["context", "system"],
    ["error", "system"],
    ["system", "system"],
  ] as const)("maps a %s step onto the %s display role", (role, displayRole) => {
    expect(toDisplayMessage(makeStep({ role })).role).toBe(displayRole);
  });

  it("keeps the step's own text when it has some", () => {
    const message = toDisplayMessage(
      makeStep({ content: "Done.", thinking_text: "hmm", error_text: "boom" }),
    );
    expect(message.content).toBe("Done.");
  });

  it("builds the body from the thought, tool calls and error when the text is empty", () => {
    const message = toDisplayMessage(
      makeStep({
        content: "  ",
        thinking_text: "Look at the config first",
        tool_calls: [
          { name: "view_file", summary: "vite.config.ts", input: "{}" },
          { name: "list_dir", summary: "", input: "{}" },
        ],
        error_text: "permission denied",
      }),
    );
    expect(message.content).toBe(
      "Look at the config first\n\nview_file — vite.config.ts\n\nlist_dir\n\npermission denied",
    );
  });

  it("turns the thought and each tool call into detail items", () => {
    const message = toDisplayMessage(
      makeStep({
        step_index: 7,
        content: "result text",
        thinking_text: "plan",
        tool_calls: [{ name: "grep", summary: "TODO", input: '{\n  "q": "TODO"\n}' }],
      }),
    );
    expect(message.items.map((item) => item.item_type)).toEqual(["Thinking", "ToolCall"]);
    expect(message.items[0]).toMatchObject({ id: "ag-7-thinking", text: "plan" });
    expect(message.items[1]).toMatchObject({
      id: "ag-7-0",
      tool_name: "grep",
      tool_summary: "TODO",
      tool_input: '{\n  "q": "TODO"\n}',
      tool_result: "result text",
    });
    expect(message.thinking_count).toBe(1);
    expect(message.tool_call_count).toBe(1);
  });

  it("records no tokens or model, since a brain transcript has none", () => {
    const message = toDisplayMessage(makeStep({ duration_ms: 1500 }));
    expect(message).toMatchObject({ model: "", tokens_raw: 0, input_tokens: 0, duration_ms: 1500 });
  });

  it("flags an error step as an error", () => {
    expect(toDisplayMessage(makeStep({ role: "error", error_text: "x" })).is_error).toBe(true);
  });
});

describe("roleStyle", () => {
  it("keeps each step type's own label and colour", () => {
    expect(roleStyle("tool")).toMatchObject({ label: "MCP Tool", className: "message--ag-tool" });
    expect(roleStyle("file").label).toBe("View File");
    expect(roleStyle("search").label).toBe("Grep Search");
    expect(roleStyle("assistant").label).toBe("Assistant");
  });
});
