import { describe, it, expect, vi } from "vitest";
import type { ReactNode } from "react";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { AntigravitySessionView } from "./AntigravitySessionView";
import type { AntigravityMessage, AntigravitySession } from "../types";
import type { ViewActionsRef } from "../hooks/useViewActions";

// The view renders rows through the Claude `MessageList`, which virtualises by
// measured height — jsdom provides none, so Virtuoso would render an empty
// viewport. Render every row instead.
vi.mock("react-virtuoso", async () => {
  const React = (await vi.importActual("react")) as typeof import("react");
  return {
    Virtuoso: React.forwardRef(function VirtuosoMock(
      {
        totalCount = 0,
        itemContent,
        context,
        className,
      }: {
        totalCount?: number;
        itemContent: (index: number, data: unknown, context: unknown) => ReactNode;
        context?: unknown;
        className?: string;
      },
      _ref: React.Ref<unknown>,
    ) {
      return (
        <div className={className}>
          {Array.from({ length: totalCount }, (_, index) => (
            // oxlint-disable-next-line react/no-array-index-key
            <div key={index}>{itemContent(index, undefined, context)}</div>
          ))}
        </div>
      );
    }),
  };
});

function makeStep(step_index: number, overrides: Partial<AntigravityMessage> = {}) {
  return {
    step_index,
    role: "assistant",
    content: `step ${step_index} body`,
    created_at: "2026-09-29T20:08:27Z",
    tool_calls: [],
    thinking_text: null,
    error_text: null,
    duration_ms: 0,
    ...overrides,
  } satisfies AntigravityMessage;
}

const STEPS: AntigravityMessage[] = [
  makeStep(0, { role: "user", content: "<USER_REQUEST>أهلاً</USER_REQUEST>", duration_ms: 3_000 }),
  makeStep(1, {
    role: "assistant",
    content: "أهلاً بك",
    tool_calls: [{ name: "view_file", summary: "Viewing file", input: '{"AbsolutePath":"x"}' }],
    thinking_text: "let me read that file first",
    duration_ms: 1_000,
  }),
  makeStep(2, { role: "tool", content: "tool result here" }),
];

function session(messages = STEPS, totalSteps = messages.length): AntigravitySession {
  return {
    folder: {
      uuid: "11111111-2222-3333-4444-555555555555",
      path: "/home/u/.gemini/antigravity-ide/brain/11111111",
      mod_time_ms: 1790776853673,
      title: "ضيف سكشن لاختيار ملف من المسار",
      flavor: "antigravity-ide",
      has_transcript: true,
      provider_id: "antigravity",
    },
    messages,
    count: messages.length,
    total_steps: totalSteps,
  };
}

describe("AntigravitySessionView", () => {
  it("renders every step body from the single read", () => {
    render(<AntigravitySessionView session={session()} />);
    for (const step of STEPS) {
      expect(screen.getByText(step.content)).toBeInTheDocument();
    }
  });

  it("has no header of its own — the toolbar carries the way back", () => {
    render(<AntigravitySessionView session={session()} />);
    expect(screen.queryByRole("button", { name: /back/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /open folder/i })).not.toBeInTheDocument();
    expect(document.querySelector(".message-list__topbar")).toBeNull();
  });

  it("keeps each step type's own border colour and role label", () => {
    render(<AntigravitySessionView session={session()} />);
    // The Claude role vocabulary ("Claude", "System") must not leak in.
    expect(document.querySelector(".message--user")).toBeTruthy();
    expect(document.querySelector(".message--claude")).toBeTruthy();
    expect(document.querySelector(".message--tool")).toBeTruthy();
    expect(screen.getByText("Assistant")).toBeInTheDocument();
    expect(screen.getByText("MCP Tool")).toBeInTheDocument();
  });

  it("shows a stats bar built from what the transcript records", () => {
    render(<AntigravitySessionView session={session()} />);
    const bars = [...document.querySelectorAll(".message__stats")].map((b) => b.textContent ?? "");
    expect(bars.some((t) => t.includes("3.0s"))).toBe(true);
    expect(bars.some((t) => t.includes("1 tool"))).toBe(true);
    expect(bars.some((t) => t.includes("1 think"))).toBe(true);
    expect(bars.some((t) => t.includes("0ms"))).toBe(false);
  });

  it("opens the detail panel, without Claude's wording leaking in", () => {
    render(<AntigravitySessionView session={session()} />);
    const detail = screen.getAllByRole("button", { name: /detail/i });
    expect(detail.length).toBe(1);
    fireEvent.click(detail[0]);
    expect(document.querySelector(".message-detail__title")?.textContent).toBe("Assistant");
    expect(document.querySelector(".message-detail__debug-hint")).toBeNull();
    fireEvent.click(screen.getByText("view_file"));
    expect(screen.getByText(/AbsolutePath/)).toBeInTheDocument();
  });

  it("registers expand/collapse into the shared toolbar ref", () => {
    const viewActionsRef = { current: {} } as ViewActionsRef;
    render(<AntigravitySessionView session={session()} viewActionsRef={viewActionsRef} />);
    expect(typeof viewActionsRef.current.expandAll).toBe("function");
    act(() => viewActionsRef.current.expandAll!());
    expect(document.querySelectorAll(".message__content--collapsed").length).toBe(0);
    act(() => viewActionsRef.current.collapseAll!());
    expect(document.querySelectorAll(".message__content--collapsed").length).toBe(STEPS.length);
  });

  it("says there is no transcript when the folder has none", () => {
    render(<AntigravitySessionView session={session([], 0)} />);
    expect(screen.getByText(/No transcript found at/i)).toBeInTheDocument();
  });

  it("says so when every step of a real transcript is empty", () => {
    // The backend leaves payload-less steps out but still counts them, so the
    // panel can tell "no transcript" from "nothing recorded".
    render(<AntigravitySessionView session={session([], 6)} />);
    expect(screen.getByText(/None of the 6 steps in this conversation/)).toBeInTheDocument();
  });

  it("falls back to the error text on a content-less error row", () => {
    const failed = [
      makeStep(0, { role: "error", content: "", error_text: "tool call failed: missing file" }),
    ];
    render(<AntigravitySessionView session={session(failed)} />);
    expect(screen.getByText(/tool call failed: missing file/)).toBeInTheDocument();
  });
});
