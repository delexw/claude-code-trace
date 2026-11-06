import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { CodexPickerSection, visibleSessions } from "./CodexPickerSection";
import { makeSession } from "./testFixtures";
import { codexProjectKey } from "./projects";

describe("CodexPickerSection", () => {
  it("combines the selected project with search", () => {
    const first = makeSession({ cwd: "/example/first" });
    const second = makeSession({ id: "second", cwd: "/example/second" });
    expect(visibleSessions([first, second], "repair", codexProjectKey(second))).toEqual([second]);
    expect(visibleSessions([first, second], "absent", codexProjectKey(second))).toEqual([]);
  });

  it("groups dates and keeps keyboard indexes aligned with the displayed order", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-10T12:00:00Z"));
    try {
      const old = makeSession({
        id: "old",
        path: "/example/old",
        title: "Old task",
        modified_at: "2025-01-01T10:00:00Z",
      });
      const today = makeSession({
        id: "today",
        path: "/example/today",
        title: "Today task",
        modified_at: "2026-01-10T10:00:00Z",
      });
      const onOpen = vi.fn();
      const sessions = visibleSessions([old, today], "");
      expect(sessions).toEqual([today, old]);
      render(
        <CodexPickerSection
          sessions={sessions}
          loading={false}
          error={null}
          searchQuery=""
          selectedIndex={0}
          onHover={vi.fn()}
          onOpen={onOpen}
          onRefresh={vi.fn()}
        />,
      );
      expect(screen.getByText("Today")).toBeInTheDocument();
      expect(screen.getByText("Older")).toBeInTheDocument();
      expect(screen.getByText("Today task").closest(".picker__session")).toHaveClass(
        "picker__session--selected",
      );
      fireEvent.click(screen.getByText("Today task"));
      expect(onOpen).toHaveBeenCalledWith(today);
    } finally {
      vi.useRealTimers();
    }
  });
  it("matches title, id, project, model, origin and archived chats", () => {
    const local = makeSession();
    const archived = makeSession({
      id: "second",
      title: "Sample task",
      cwd: "/work/another",
      source: "WSL: SampleLinux",
      archived: true,
    });
    for (const query of ["REPAIR", "sample-session", "/work/sample", "gpt-sample"])
      expect(visibleSessions([local], query)).toEqual([local]);
    for (const query of ["another", "samplelinux", "archived"])
      expect(visibleSessions([local, archived], query)).toEqual([archived]);
    expect(visibleSessions([local], "absent")).toEqual([]);
  });

  it("shows origins and archive status, opens cards, and offers refresh", () => {
    const session = makeSession({ source: "WSL: SampleLinux", archived: true });
    const onOpen = vi.fn();
    const onRefresh = vi.fn();
    const onHover = vi.fn();
    render(
      <CodexPickerSection
        sessions={[session]}
        loading={false}
        error={null}
        searchQuery=""
        selectedIndex={0}
        onHover={onHover}
        onOpen={onOpen}
        onRefresh={onRefresh}
      />,
    );
    expect(screen.getByText("WSL: SampleLinux")).toBeInTheDocument();
    expect(screen.getByText("Archived")).toBeInTheDocument();
    expect(screen.getByText(session.title).closest(".picker__session")).toHaveClass(
      "picker__session--selected",
    );
    fireEvent.mouseEnter(screen.getByText(session.title));
    fireEvent.click(screen.getByRole("button", { name: /Detail/ }));
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onHover).toHaveBeenCalledWith(0);
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onOpen).toHaveBeenCalledWith(session);
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("shows live activity, turns, duration and recorded repository/access metadata", () => {
    render(
      <CodexPickerSection
        sessions={[
          makeSession({
            is_ongoing: true,
            turn_count: 4,
            duration_ms: 65000,
            git_branch: "feature/example",
            permission_mode: "on-request",
            sandbox_policy: "workspace-write",
          }),
        ]}
        loading={false}
        error={null}
        searchQuery=""
        selectedIndex={0}
        onHover={vi.fn()}
        onOpen={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByTitle("Codex is working").closest(".picker__session")).toHaveClass(
      "picker__session--ongoing",
    );
    expect(screen.getByText("4 turns")).toBeInTheDocument();
    expect(screen.getByText("1m 5s")).toBeInTheDocument();
    expect(screen.getByTitle("Git branch")).toHaveTextContent("feature/example");
    expect(screen.getByTitle("Approval policy")).toHaveTextContent("on-request");
    expect(screen.getByTitle("Sandbox policy")).toHaveTextContent("workspace-write");
  });

  it("shows a session's estimated dollars and recorded tokens like Claude cards", () => {
    render(
      <CodexPickerSection
        sessions={[makeSession({ cost_usd: 1.25, cost_partial: true, total_tokens: 1234 })]}
        loading={false}
        error={null}
        searchQuery=""
        selectedIndex={0}
        onHover={vi.fn()}
        onOpen={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$1.25+");
    expect(screen.getByLabelText("Estimated API token cost")).toHaveClass(
      "picker__session-stat--cost",
    );
    expect(screen.getByText("1.2k tok")).toBeInTheDocument();
  });
});
