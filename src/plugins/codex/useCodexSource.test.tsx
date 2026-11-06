import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useLayoutEffect, useState, type ReactNode } from "react";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import type { SourceContext, SourceRuntime } from "../types";
import { makeMessage, makeSession, makeWindow } from "./testFixtures";

const mockInvoke = vi.fn();
vi.mock("../../lib/invoke", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
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

import { useCodexSource } from "./useCodexSource";

let latest: SourceRuntime;
const actions = { current: {} };

function Harness({
  enabled = true,
  searchQuery = "",
}: {
  enabled?: boolean;
  searchQuery?: string;
}) {
  const [view, setView] = useState<string | null>(null);
  const ctx: SourceContext = {
    enabled,
    activeView: view,
    openView: setView,
    goToPicker: () => setView(null),
    setPickerIndex: vi.fn(),
    searchQuery,
    viewActionsRef: actions,
    fontScale: 1,
    recapPreview: false,
    settings: null,
    openSettings: vi.fn(),
  };
  const source = useCodexSource(ctx);
  useLayoutEffect(() => {
    latest = source;
  });
  return (
    <>
      {view ? (
        <>
          {source.views[view].render()}
          {source.views[view].infoBar}
        </>
      ) : (
        <>
          {source.picker.headerStats}
          {source.picker.render({ selectedIndex: null, onHover: vi.fn() })}
        </>
      )}
    </>
  );
}

function serve(
  messages = [makeMessage({ role: "user", content: "Check the sample build" }), makeMessage()],
) {
  mockInvoke.mockImplementation((command: string, args?: { start?: number; limit?: number }) => {
    if (command === "list_codex_sessions") return Promise.resolve([makeSession()]);
    if (command === "load_codex_session") {
      const from = args?.start ?? 0;
      return Promise.resolve(
        makeWindow({
          messages: messages.slice(from, from + (args?.limit ?? messages.length)),
          start: from,
          count: messages.length,
          roles: messages.map((message) => message.role),
        }),
      );
    }
    return Promise.resolve(undefined);
  });
}

async function start() {
  await act(() => latest.onSettingsLoaded!(null));
}

async function open() {
  await start();
  fireEvent.click(await screen.findByText("Repair the sample build"));
  await screen.findByText("The sample build now passes.");
}

describe("useCodexSource", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    serve();
  });
  afterEach(() => vi.useRealTimers());

  it("lists and opens Codex chats using a bounded message window and Codex labels", async () => {
    render(<Harness />);
    await open();
    expect(screen.getByText("Check the sample build")).toBeInTheDocument();
    expect(screen.getAllByText("Codex").length).toBeGreaterThan(0);
    expect(screen.queryByText("Claude")).toBeNull();
    expect(mockInvoke).toHaveBeenCalledWith("load_codex_session", {
      path: makeSession().path,
      limit: 0,
    });
    expect(mockInvoke).toHaveBeenCalledWith("load_codex_session", {
      path: makeSession().path,
      start: 0,
      limit: 80,
    });
    act(() => latest.views.list.keyMap.Enter());
    expect(await screen.findByText("The sample build now passes.")).toBeInTheDocument();
    expect(screen.queryByText(/debug log/i)).toBeNull();
  });

  it("does not read local chats while disabled", async () => {
    render(<Harness enabled={false} />);
    await start();
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(latest.picker.count).toBe(0);
  });

  it("uses the same search results for keyboard selection and displayed cards", async () => {
    mockInvoke.mockResolvedValue([
      makeSession(),
      makeSession({
        id: "other",
        path: "/sample/other.jsonl",
        title: "Add sample tests",
        cwd: "/work/other",
      }),
    ]);
    render(<Harness searchQuery="other" />);
    await start();
    expect(latest.picker.count).toBe(1);
    expect(screen.getByText("Add sample tests")).toBeInTheDocument();
    expect(screen.queryByText("Repair the sample build")).toBeNull();
  });

  it("totals only search-matched sessions in the shared picker header", async () => {
    mockInvoke.mockResolvedValue([
      makeSession({ cost_usd: 10, total_tokens: 1000 }),
      makeSession({
        id: "other",
        path: "/sample/other.jsonl",
        title: "Matched sample",
        cost_usd: 2.5,
        total_tokens: 500,
      }),
    ]);
    const rendered = render(<Harness />);
    await start();
    expect(screen.getByLabelText("Estimated Codex API token cost")).toHaveTextContent("$12.50");
    rendered.rerender(<Harness searchQuery="Matched" />);
    expect(screen.getByLabelText("Estimated Codex API token cost")).toHaveTextContent("$2.50");
    expect(screen.getByText("500 tok", { selector: ".picker__total-tokens" })).toBeInTheDocument();
  });

  it("reports discovery and opening failures in the picker", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("sample directory unavailable"));
    render(<Harness />);
    await start();
    expect(screen.getByRole("alert")).toHaveTextContent("sample directory unavailable");
    serve();
    await start();
    mockInvoke.mockRejectedValueOnce(new Error("sample file removed"));
    fireEvent.click(screen.getByText("Repair the sample build"));
    expect(await screen.findByRole("alert")).toHaveTextContent("sample file removed");
    expect(latest.hasOpenSession).toBe(false);
  });

  it("keeps the newest selection when an earlier open request finishes later", async () => {
    let finishFirst!: (value: ReturnType<typeof makeWindow>) => void;
    const second = makeSession({
      id: "second",
      path: "/sample/second.jsonl",
      title: "Second sample chat",
    });
    mockInvoke.mockImplementation((command: string, args?: { path: string; limit?: number }) => {
      if (command === "list_codex_sessions") return Promise.resolve([makeSession(), second]);
      if (args?.path === makeSession().path)
        return new Promise((resolve) => {
          finishFirst = resolve;
        });
      return Promise.resolve(
        makeWindow({
          messages: args?.limit === 0 ? [] : [makeMessage({ content: "Second sample response" })],
        }),
      );
    });
    render(<Harness />);
    await start();
    fireEvent.click(screen.getByText("Repair the sample build"));
    fireEvent.click(screen.getByText("Second sample chat"));
    expect(await screen.findByText("Second sample response")).toBeInTheDocument();
    await act(async () => finishFirst(makeWindow()));
    expect(screen.getByText("Second sample response")).toBeInTheDocument();
    expect(screen.queryByText("The sample build now passes.")).toBeNull();
  });

  it("polls the active chat and stops reading it after the plugin is disabled", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const rendered = render(<Harness />);
    await open();
    mockInvoke.mockImplementation((command: string, args?: { limit?: number }) =>
      Promise.resolve(
        command === "load_codex_session"
          ? makeWindow({
              revision: "updated",
              messages: args?.limit === 0 ? [] : [makeMessage({ content: "New sample output" })],
            })
          : [makeSession()],
      ),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2500);
    });
    expect(screen.getByText("New sample output")).toBeInTheDocument();
    rendered.rerender(<Harness enabled={false} />);
    mockInvoke.mockClear();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10000);
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("clears transient refresh errors and updates activity without downloading unchanged bodies", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    mockInvoke.mockImplementation((command: string, args?: { limit?: number }) =>
      Promise.resolve(
        command === "list_codex_sessions"
          ? [makeSession()]
          : makeWindow({ ongoing: true, messages: args?.limit === 0 ? [] : [makeMessage()] }),
      ),
    );
    const rendered = render(<Harness />);
    await open();
    expect(rendered.container.querySelector(".ongoing-dots")).not.toBeNull();
    mockInvoke.mockRejectedValueOnce(new Error("sample connection interrupted"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    expect(screen.getByRole("alert")).toHaveTextContent("sample connection interrupted");
    mockInvoke.mockResolvedValue(makeWindow({ ongoing: false, messages: [] }));
    mockInvoke.mockClear();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(rendered.container.querySelector(".ongoing-dots")).toBeNull();
    expect(mockInvoke).toHaveBeenCalledOnce();
    expect(mockInvoke).toHaveBeenCalledWith("load_codex_session", {
      path: makeSession().path,
      limit: 0,
    });
    expect(screen.getByText("The sample build now passes.")).toBeInTheDocument();
  });

  it("refreshes cost metadata even when the recorded message revision is unchanged", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    render(<Harness />);
    await open();
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$0.25");
    mockInvoke.mockResolvedValue(makeWindow({ cost_usd: 1.5, total_tokens: 1500, messages: [] }));
    mockInvoke.mockClear();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$1.50");
    expect(screen.getByText("1.5k tokens")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledOnce();
    expect(screen.getByText("The sample build now passes.")).toBeInTheDocument();
  });

  it("loads an older message on demand when navigating outside the initial window", async () => {
    serve(
      Array.from({ length: 200 }, (_, index) =>
        makeMessage({ content: `Sample message ${index}` }),
      ),
    );
    render(<Harness />);
    await start();
    fireEvent.click(screen.getByText("Repair the sample build"));
    expect(await screen.findByText("Sample message 199")).toBeInTheDocument();
    expect(screen.queryByText("Sample message 0")).toBeNull();
    act(() => latest.views.list.keyMap.g());
    act(() => latest.views.list.keyMap.Enter());
    expect(await screen.findByText("Sample message 0")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("load_codex_session", {
      path: makeSession().path,
      start: 0,
      limit: 80,
    });
  });

  it("refreshes the selected detail and rejects an older window arriving after the refresh", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const messages = Array.from({ length: 200 }, (_, index) =>
      makeMessage({ content: `Sample message ${index}` }),
    );
    serve(messages);
    render(<Harness />);
    await start();
    fireEvent.click(screen.getByText("Repair the sample build"));
    await screen.findByText("Sample message 199");
    let finishOld!: (value: ReturnType<typeof makeWindow>) => void;
    let holdFirstWindow = true;
    mockInvoke.mockImplementation((_command: string, args: { start?: number; limit?: number }) => {
      if (args.limit !== 0 && holdFirstWindow) {
        holdFirstWindow = false;
        return new Promise((resolve) => {
          finishOld = resolve;
        });
      }
      const from = args.start ?? 0;
      return Promise.resolve(
        makeWindow({
          revision: "updated",
          count: 200,
          roles: messages.map((message) => message.role),
          start: from,
          messages:
            args.limit === 0
              ? []
              : messages
                  .slice(from, from + (args.limit ?? 80))
                  .map((message) => makeMessage({ content: `Updated ${message.content}` })),
        }),
      );
    });
    act(() => latest.views.list.keyMap.g());
    act(() => latest.views.list.keyMap.Enter());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2100);
    });
    expect(screen.getByText("Updated Sample message 0")).toBeInTheDocument();
    await act(async () =>
      finishOld(
        makeWindow({ count: 200, messages: [makeMessage({ content: "Stale sample response" })] }),
      ),
    );
    expect(screen.getByText("Updated Sample message 0")).toBeInTheDocument();
    expect(screen.queryByText("Stale sample response")).toBeNull();
  });

  it("accepts bootstrap settings before the enabled render arrives", async () => {
    const rendered = render(<Harness enabled={false} />);
    let loading!: Promise<void>;
    act(() => {
      loading = latest.onSettingsLoaded!({
        plugins: [{ id: "codex", enabled: true, label: "Codex", can_disable: true }],
      } as never) as Promise<void>;
      rendered.rerender(<Harness enabled />);
    });
    await act(async () => loading);
    expect(screen.getByText("Repair the sample build")).toBeInTheDocument();
  });

  it("ignores an opening request after the plugin is disabled", async () => {
    let finish!: (value: ReturnType<typeof makeWindow>) => void;
    const rendered = render(<Harness />);
    await start();
    mockInvoke.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    fireEvent.click(screen.getByText("Repair the sample build"));
    rendered.rerender(<Harness enabled={false} />);
    await act(async () => finish(makeWindow()));
    expect(latest.hasOpenSession).toBe(false);
  });

  it("shows an empty-chat notice and keeps navigation in bounds", async () => {
    serve([]);
    render(<Harness />);
    await start();
    fireEvent.click(screen.getByText("Repair the sample build"));
    await waitFor(() => expect(latest.hasOpenSession).toBe(true));
    expect(screen.getByText("This Codex chat has no recorded messages yet.")).toBeInTheDocument();
    act(() => {
      latest.views.list.keyMap.j();
      latest.views.list.keyMap.Enter();
    });
    expect(screen.getByText("This Codex chat has no recorded messages yet.")).toBeInTheDocument();
  });
});
