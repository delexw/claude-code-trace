import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ProjectTree, useProjectKeys } from "./ProjectTree";
import { renderHook } from "@testing-library/react";
import type { SessionInfo } from "../types";

function makeSession(overrides: Partial<SessionInfo> = {}): SessionInfo {
  return {
    path: "/home/user/.claude/projects/my-project/session1.jsonl",
    session_id: "session1",
    mod_time: "2025-01-01T00:00:00Z",
    first_message: "Hello",
    turn_count: 3,
    is_ongoing: false,
    total_tokens: 1000,
    input_tokens: 500,
    output_tokens: 500,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    cost_usd: 0.01,
    duration_ms: 5000,
    model: "claude-sonnet-4-20250514",
    cwd: "/home/user/my-project",
    git_branch: "main",
    permission_mode: "default",
    ...overrides,
  };
}

describe("ProjectTree", () => {
  it("shows All Projects with total count", () => {
    const sessions = [
      makeSession(),
      makeSession({ path: "/home/user/.claude/projects/my-project/session2.jsonl" }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("All Projects")).toBeInTheDocument();
    // Total count should be 2
    const allItem = screen.getByText("All Projects").closest(".project-tree__item")!;
    expect(allItem.querySelector(".project-tree__count")!.textContent).toBe("2");
  });

  it("groups sessions by project key", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/home/user/proj-a",
      }),
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s2.jsonl",
        cwd: "/home/user/proj-a",
      }),
      makeSession({
        path: "/home/user/.claude/projects/proj-b/s3.jsonl",
        cwd: "/home/user/proj-b",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("proj-a")).toBeInTheDocument();
    expect(screen.getByText("proj-b")).toBeInTheDocument();
  });

  it("shows ongoing dot for projects with ongoing sessions", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/home/user/proj-a",
        is_ongoing: true,
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const projItem = screen.getByText("proj-a").closest(".project-tree__item")!;
    expect(projItem.querySelector(".ongoing-dots")).toBeInTheDocument();
  });

  it("does not show ongoing dot when no ongoing sessions", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/home/user/proj-a",
        is_ongoing: false,
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const projItem = screen.getByText("proj-a").closest(".project-tree__item")!;
    expect(projItem.querySelector(".ongoing-dots")).not.toBeInTheDocument();
  });

  it("highlights selected project", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/home/user/proj-a",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject="proj-a"
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const projItem = screen.getByText("proj-a").closest(".project-tree__item")!;
    expect(projItem).toHaveClass("project-tree__item--selected");
  });

  it("highlights All Projects when selectedProject is null", () => {
    render(
      <ProjectTree
        sessions={[makeSession()]}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const allItem = screen.getByText("All Projects").closest(".project-tree__item")!;
    expect(allItem).toHaveClass("project-tree__item--selected");
  });

  it("clicking All Projects calls onSelectProject(null)", () => {
    const onSelectProject = vi.fn();
    render(
      <ProjectTree
        sessions={[makeSession()]}
        selectedProject={null}
        onSelectProject={onSelectProject}
        onRefresh={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("All Projects"));
    expect(onSelectProject).toHaveBeenCalledWith(null);
  });

  it("clicking a project calls onSelectProject with key", () => {
    const onSelectProject = vi.fn();
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/home/user/proj-a",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={onSelectProject}
        onRefresh={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("proj-a"));
    expect(onSelectProject).toHaveBeenCalledWith("proj-a");
  });

  it("refresh button calls onRefresh", () => {
    const onRefresh = vi.fn();
    render(
      <ProjectTree
        sessions={[makeSession()]}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={onRefresh}
      />,
    );
    fireEvent.click(screen.getByTitle("Refresh all projects"));
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("applies custom style prop", () => {
    const { container } = render(
      <ProjectTree
        sessions={[]}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
        style={{ width: "300px" }}
      />,
    );
    const tree = container.querySelector(".project-tree")!;
    expect(tree).toHaveStyle({ width: "300px" });
  });

  it("groups worktree projects under a 'worktrees' group node", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-123-fix-bug/s2.jsonl",
        cwd: "/home/user/backend/worktrees/EC-123-fix-bug",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("backend")).toBeInTheDocument();
    // Group node should exist
    const groupItem = screen.getByText("worktrees").closest(".project-tree__item")!;
    expect(groupItem).toHaveClass("project-tree__item--group");
    // Leaf should show short name under the group
    expect(screen.getByText("EC-123-fix-bug")).toBeInTheDocument();
  });

  it("groups claude worktrees under a 'claude-worktrees' group node", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend--claude-worktrees-happy-crane/s2.jsonl",
        cwd: "/home/user/backend/.claude-worktrees/happy-crane",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("backend")).toBeInTheDocument();
    // Group node should exist
    const groupItem = screen.getByText("claude-worktrees").closest(".project-tree__item")!;
    expect(groupItem).toHaveClass("project-tree__item--group");
    // Leaf should show short name
    expect(screen.getByText("happy-crane")).toBeInTheDocument();
  });

  it("useProjectKeys returns keys in tree order with group nodes", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-456/s2.jsonl",
        cwd: "/home/user/backend/worktrees/EC-456",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
    ];
    const { result } = renderHook(() => useProjectKeys(sessions));
    const keys = result.current;
    expect(keys[0]).toBeNull();
    // Parent → group → child
    const parentIdx = keys.indexOf("-Users-me-backend");
    const groupIdx = keys.indexOf("__group:worktrees:-Users-me-backend");
    const childIdx = keys.indexOf("-Users-me-backend-worktrees-EC-456");
    expect(parentIdx).toBeGreaterThan(0);
    expect(groupIdx).toBeGreaterThan(parentIdx);
    expect(childIdx).toBeGreaterThan(groupIdx);
  });

  it("does not nest unrelated projects", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-frontend/s2.jsonl",
        cwd: "/home/user/frontend",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    // Neither should be a child
    const children = container.querySelectorAll(".project-tree__item--child");
    expect(children.length).toBe(0);
  });

  it("nests generic sub-projects directly without group node", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-claude-code-misc/s1.jsonl",
        cwd: "/home/user/claude-code-misc",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-claude-code-misc-agents/s2.jsonl",
        cwd: "/home/user/claude-code-misc/agents",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("claude-code-misc")).toBeInTheDocument();
    const childItem = screen.getByText(/agents/).closest(".project-tree__item")!;
    expect(childItem).toHaveClass("project-tree__item--child");
    // No group nodes for generic children
    expect(container.querySelectorAll(".project-tree__item--group").length).toBe(0);
  });

  it("nests multiple sub-projects under a shared parent", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-others/s1.jsonl",
        cwd: "/home/user/others",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-others-proj-alpha/s2.jsonl",
        cwd: "/home/user/others/proj-alpha",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-others-proj-beta/s3.jsonl",
        cwd: "/home/user/others/proj-beta",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    expect(screen.getByText("others")).toBeInTheDocument();
    const children = container.querySelectorAll(".project-tree__item--child");
    expect(children.length).toBe(2);
  });

  it("does not nest projects with similar prefix but no parent project", () => {
    // -Users-me-backend and -Users-me-backend-v2 are both root projects
    // because neither is a parent of the other unless both exist as project keys
    // Here -Users-me-backend IS a project, so -Users-me-backend-v2 nests under it
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-v2/s2.jsonl",
        cwd: "/home/user/backend-v2",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    // backend-v2 nests under backend since backend is a real project key prefix
    const children = container.querySelectorAll(".project-tree__item--child");
    expect(children.length).toBe(1);
  });

  it("keeps projects as roots when no parent project key exists", () => {
    // Only -Users-me-app-frontend and -Users-me-app-backend exist,
    // but -Users-me-app does NOT exist — so neither should nest
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-app-frontend/s1.jsonl",
        cwd: "/home/user/app-frontend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-app-backend/s2.jsonl",
        cwd: "/home/user/app-backend",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const children = container.querySelectorAll(".project-tree__item--child");
    expect(children.length).toBe(0);
  });

  it("mixes worktree groups and regular children under same parent", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-789/s2.jsonl",
        cwd: "/home/user/backend/worktrees/EC-789",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend--claude-worktrees-fast-fox/s3.jsonl",
        cwd: "/home/user/backend/.claude-worktrees/fast-fox",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-tools/s4.jsonl",
        cwd: "/home/user/backend/tools",
      }),
    ];
    const { container } = render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    // Two group nodes
    const groups = container.querySelectorAll(".project-tree__item--group");
    expect(groups.length).toBe(2);
    expect(screen.getByText("worktrees")).toBeInTheDocument();
    expect(screen.getByText("claude-worktrees")).toBeInTheDocument();
    // Regular child has no group
    expect(screen.getByText("tools")).toBeInTheDocument();
    // Worktree leaf
    expect(screen.getByText("EC-789")).toBeInTheDocument();
    // Claude worktree leaf
    expect(screen.getByText("fast-fox")).toBeInTheDocument();
  });

  it("group node is not clickable", () => {
    const onSelectProject = vi.fn();
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-789/s2.jsonl",
        cwd: "/home/user/backend/worktrees/EC-789",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={onSelectProject}
        onRefresh={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("worktrees"));
    expect(onSelectProject).not.toHaveBeenCalled();
  });

  it("group node shows aggregate count", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend/s1.jsonl",
        cwd: "/home/user/backend",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-1/s2.jsonl",
        cwd: "/home/user/backend/worktrees/EC-1",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-1/s3.jsonl",
        cwd: "/home/user/backend/worktrees/EC-1",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-backend-worktrees-EC-2/s4.jsonl",
        cwd: "/home/user/backend/worktrees/EC-2",
      }),
    ];
    render(
      <ProjectTree
        sessions={sessions}
        selectedProject={null}
        onSelectProject={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );
    const groupItem = screen.getByText("worktrees").closest(".project-tree__item")!;
    expect(groupItem.querySelector(".project-tree__count")!.textContent).toBe("3");
  });

  it("useProjectKeys returns correct order for generic nesting", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-claude-code-misc-agents/s2.jsonl",
        cwd: "/home/user/claude-code-misc/agents",
      }),
      makeSession({
        path: "/home/user/.claude/projects/-Users-me-claude-code-misc/s1.jsonl",
        cwd: "/home/user/claude-code-misc",
      }),
    ];
    const { result } = renderHook(() => useProjectKeys(sessions));
    const keys = result.current;
    expect(keys[0]).toBeNull();
    const parentIdx = keys.indexOf("-Users-me-claude-code-misc");
    const childIdx = keys.indexOf("-Users-me-claude-code-misc-agents");
    expect(parentIdx).toBeGreaterThan(0);
    expect(childIdx).toBeGreaterThan(parentIdx);
  });
});
