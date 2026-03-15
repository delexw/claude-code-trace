import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { TeamBoard } from "./TeamBoard";
import type { TeamSnapshot } from "../types";

function makeTeam(overrides: Partial<TeamSnapshot> = {}): TeamSnapshot {
  return {
    name: "Alpha Team",
    description: "A test team",
    tasks: [],
    members: [],
    member_colors: {},
    member_ongoing: {},
    deleted: false,
    ...overrides,
  };
}

describe("TeamBoard", () => {
  it("shows 'No active teams' when teams array is empty", () => {
    render(<TeamBoard teams={[]} />);
    expect(screen.getByText("No active teams")).toBeInTheDocument();
  });

  it("shows 'No active teams' when all teams are deleted", () => {
    const teams = [makeTeam({ deleted: true }), makeTeam({ deleted: true, name: "Beta" })];
    render(<TeamBoard teams={teams} />);
    expect(screen.getByText("No active teams")).toBeInTheDocument();
  });

  it("filters out deleted teams", () => {
    const teams = [
      makeTeam({ name: "Active Team", deleted: false }),
      makeTeam({ name: "Deleted Team", deleted: true }),
    ];
    render(<TeamBoard teams={teams} />);
    expect(screen.getByText("Active Team")).toBeInTheDocument();
    expect(screen.queryByText("Deleted Team")).not.toBeInTheDocument();
  });

  it("renders team name and description", () => {
    const teams = [makeTeam({ name: "My Team", description: "Does cool stuff" })];
    render(<TeamBoard teams={teams} />);
    expect(screen.getByText("My Team")).toBeInTheDocument();
    expect(screen.getByText("Does cool stuff")).toBeInTheDocument();
  });

  it("renders members with colored dots", () => {
    const teams = [
      makeTeam({
        members: ["alice", "bob"],
        member_colors: { alice: "blue", bob: "red" },
      }),
    ];
    render(<TeamBoard teams={teams} />);
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText("Members (2)")).toBeInTheDocument();
  });

  it("shows ongoing indicator for ongoing members", () => {
    const teams = [
      makeTeam({
        members: ["alice"],
        member_colors: { alice: "blue" },
        member_ongoing: { alice: true },
      }),
    ];
    render(<TeamBoard teams={teams} />);
    const memberEl = screen.getByText("alice").closest(".team-member")!;
    expect(memberEl.querySelector(".team-member__ongoing")).toBeInTheDocument();
  });

  it("does not show ongoing indicator for non-ongoing members", () => {
    const teams = [
      makeTeam({
        members: ["alice"],
        member_colors: { alice: "blue" },
        member_ongoing: { alice: false },
      }),
    ];
    render(<TeamBoard teams={teams} />);
    const memberEl = screen.getByText("alice").closest(".team-member")!;
    expect(memberEl.querySelector(".team-member__ongoing")).not.toBeInTheDocument();
  });

  it("renders tasks with status icons and badges", () => {
    const teams = [
      makeTeam({
        tasks: [
          { id: "1", subject: "Fix bug", status: "completed", owner: "alice" },
          { id: "2", subject: "Add feature", status: "in_progress", owner: "bob" },
          { id: "3", subject: "Write docs", status: "pending", owner: "" },
        ],
      }),
    ];
    render(<TeamBoard teams={teams} />);
    expect(screen.getByText("Fix bug")).toBeInTheDocument();
    expect(screen.getByText("Add feature")).toBeInTheDocument();
    expect(screen.getByText("Write docs")).toBeInTheDocument();
    expect(screen.getByText("completed")).toBeInTheDocument();
    expect(screen.getByText("in_progress")).toBeInTheDocument();
    expect(screen.getByText("pending")).toBeInTheDocument();
    expect(screen.getByText("Tasks (3)")).toBeInTheDocument();
  });
});
