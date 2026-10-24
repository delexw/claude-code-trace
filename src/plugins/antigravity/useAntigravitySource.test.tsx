import { describe, it, expect, vi, beforeEach } from "vitest";
import { useLayoutEffect, useState, type ReactNode } from "react";
import { render, screen, fireEvent, act } from "@testing-library/react";
import type { SourceContext, SourceRuntime } from "../types";
import { makeFolder, makeStep } from "./testFixtures";
import type { AntigravityMessage } from "./types";

const mockInvoke = vi.fn();
vi.mock("../../lib/invoke", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("react-virtuoso", () => ({
  Virtuoso: ({
    totalCount = 0,
    itemContent,
    context,
  }: {
    totalCount?: number;
    itemContent: (index: number, data: unknown, context: unknown) => ReactNode;
    context?: unknown;
  }) => (
    <div>
      {Array.from({ length: totalCount }, (_, index) => (
        // oxlint-disable-next-line react/no-array-index-key
        <div key={index}>{itemContent(index, undefined, context)}</div>
      ))}
    </div>
  ),
}));

import { useAntigravitySource } from "./useAntigravitySource";

const latest: { runtime: SourceRuntime | null } = { runtime: null };
const runtime = () => latest.runtime!;

function Harness() {
  const [view, setView] = useState<string | null>(null);
  const ctx: SourceContext = {
    enabled: true,
    activeView: view,
    openView: setView,
    goToPicker: () => setView(null),
    setPickerIndex: vi.fn(),
    searchQuery: "",
    viewActionsRef: { current: {} },
    fontScale: 1,
    recapPreview: false,
    settings: null,
    openSettings: vi.fn(),
  };
  const source = useAntigravitySource(ctx);
  useLayoutEffect(() => {
    latest.runtime = source;
  });
  return (
    <>
      {view
        ? source.views[view].render()
        : source.picker.render({ selectedIndex: null, onHover: vi.fn() })}
      {view && source.views[view].infoBar}
    </>
  );
}

function serve(steps: AntigravityMessage[], totalRows = steps.length) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_antigravity_folders") return Promise.resolve([makeFolder()]);
    if (cmd === "load_antigravity_session")
      return Promise.resolve({
        messages: steps,
        count: steps.length,
        start: 0,
        roles: steps.map((s) => s.role),
        total_rows: totalRows,
      });
    return Promise.resolve(undefined);
  });
}

async function openFirstConversation() {
  render(<Harness />);
  await act(() => runtime().onSettingsLoaded!(null));
  fireEvent.click(await screen.findByText("Fix the flaky build"));
}

describe("useAntigravitySource", () => {
  beforeEach(() => vi.clearAllMocks());

  it("opens a conversation with each step's own label and colour", async () => {
    serve([
      makeStep({ step_index: 0, role: "user", content: "Why is CI red?" }),
      makeStep({ step_index: 1, role: "tool", content: "ran it" }),
      makeStep({ step_index: 2, role: "assistant", content: "Fixed." }),
    ]);
    await openFirstConversation();

    expect(await screen.findByText("Why is CI red?")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("load_antigravity_session", {
      path: makeFolder().path,
    });
    const tool = screen.getByText("MCP Tool");
    expect(tool).toHaveClass("message__role");
    expect(tool.closest(".message")).toHaveClass("message--ag-tool");
    expect(screen.getByText("Assistant")).toBeInTheDocument();
    expect(screen.queryByText("Claude")).toBeNull();
  });

  it("says when the conversation has no transcript", async () => {
    serve([], 0);
    await openFirstConversation();
    expect(await screen.findByText(/No transcript found at/)).toBeInTheDocument();
  });

  it("says when no step recorded any text", async () => {
    serve([], 4);
    await openFirstConversation();
    expect(
      await screen.findByText("None of the 4 steps in this conversation recorded any text."),
    ).toBeInTheDocument();
  });

  it("shows a step's detail without Claude-specific wording", async () => {
    serve([makeStep({ role: "file", content: "" })]);
    await openFirstConversation();
    await screen.findByText("View File");

    act(() => runtime().views.list.keyMap.Enter());

    expect(
      await screen.findByText("This step records no text, tool call or result."),
    ).toBeInTheDocument();
    expect(screen.getAllByText("View File").length).toBeGreaterThan(0);
  });

  it("stays on the picker when the conversation fails to load", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "list_antigravity_folders"
        ? Promise.resolve([makeFolder()])
        : Promise.reject(new Error("gone")),
    );
    await openFirstConversation();

    await act(() => Promise.resolve());
    expect(screen.getByText("Fix the flaky build")).toBeInTheDocument();
    expect(runtime().hasOpenSession).toBe(false);
    error.mockRestore();
  });
});
