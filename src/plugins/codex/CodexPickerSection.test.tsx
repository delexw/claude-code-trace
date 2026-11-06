import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { CodexPickerSection, visibleSessions } from "./CodexPickerSection";
import { makeSession } from "./testFixtures";

describe("CodexPickerSection", () => {
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

  it("shows a session's estimated dollars and recorded tokens like Claude cards", () => {
    render(
      <CodexPickerSection
        sessions={[makeSession({ cost_usd: 1.25, total_tokens: 1234 })]}
        loading={false}
        error={null}
        searchQuery=""
        selectedIndex={0}
        onHover={vi.fn()}
        onOpen={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Estimated API token cost")).toHaveTextContent("$1.25");
    expect(screen.getByLabelText("Estimated API token cost")).toHaveClass(
      "picker__session-stat--cost",
    );
    expect(screen.getByText("1.2k tok")).toBeInTheDocument();
  });
});
