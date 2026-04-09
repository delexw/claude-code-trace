import { describe, it, expect } from "vitest";
import type { SessionInfo } from "./types";
import {
  buildProjectNodes,
  buildTree,
  treeNodeCmp,
  flattenTree,
  detectWorktreeKind,
  worktreeLeafName,
  buildFlatItems,
  type FlatItem,
} from "./projectTree";

function omitExpandFields(items: FlatItem[]): Omit<FlatItem, "hasChildren" | "isExpanded">[] {
  return items.map(({ hasChildren: _h, isExpanded: _e, ...rest }) => rest);
}

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

describe("buildProjectNodes", () => {
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
    const nodes = buildProjectNodes(sessions);
    expect(nodes).toHaveLength(2);
    const a = nodes.find((n) => n.key === "proj-a")!;
    expect(a.sessionCount).toBe(2);
    expect(a.name).toBe("proj-a");
  });

  it("tracks ongoing status", () => {
    const sessions = [
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s1.jsonl",
        cwd: "/x/proj-a",
        is_ongoing: false,
      }),
      makeSession({
        path: "/home/user/.claude/projects/proj-a/s2.jsonl",
        cwd: "/x/proj-a",
        is_ongoing: true,
      }),
    ];
    const nodes = buildProjectNodes(sessions);
    expect(nodes[0].hasOngoing).toBe(true);
  });

  it("sorts by name", () => {
    const sessions = [
      makeSession({ path: "/home/user/.claude/projects/zebra/s.jsonl", cwd: "/x/zebra" }),
      makeSession({ path: "/home/user/.claude/projects/alpha/s.jsonl", cwd: "/x/alpha" }),
    ];
    const nodes = buildProjectNodes(sessions);
    expect(nodes[0].name).toBe("alpha");
    expect(nodes[1].name).toBe("zebra");
  });
});

describe("treeNodeCmp", () => {
  it("compares by node name", () => {
    const a = {
      node: { name: "alpha", key: "", sessionCount: 0, hasOngoing: false },
      children: [],
    };
    const b = { node: { name: "beta", key: "", sessionCount: 0, hasOngoing: false }, children: [] };
    expect(treeNodeCmp(a, b)).toBeLessThan(0);
    expect(treeNodeCmp(b, a)).toBeGreaterThan(0);
  });
});

describe("buildTree", () => {
  it("nests child under parent by key prefix", () => {
    const nodes = [
      { name: "backend", key: "-Users-me-backend", sessionCount: 1, hasOngoing: false },
      { name: "tools", key: "-Users-me-backend-tools", sessionCount: 1, hasOngoing: false },
    ];
    const roots = buildTree(nodes);
    expect(roots).toHaveLength(1);
    expect(roots[0].node.key).toBe("-Users-me-backend");
    expect(roots[0].children).toHaveLength(1);
    expect(roots[0].children[0].node.key).toBe("-Users-me-backend-tools");
  });

  it("keeps unrelated projects as roots", () => {
    const nodes = [
      { name: "backend", key: "-Users-me-backend", sessionCount: 1, hasOngoing: false },
      { name: "frontend", key: "-Users-me-frontend", sessionCount: 1, hasOngoing: false },
    ];
    const roots = buildTree(nodes);
    expect(roots).toHaveLength(2);
  });

  it("creates virtual intermediate node for -- segments between parent and worktree", () => {
    const nodes = [
      { name: "sp-03bf9f55", key: "-Users-me--sp-03bf9f55", sessionCount: 1, hasOngoing: false },
      {
        name: "wt",
        key: "-Users-me--sp-03bf9f55-translation-service--claude-worktrees-main",
        sessionCount: 1,
        hasOngoing: false,
      },
    ];
    const roots = buildTree(nodes);
    expect(roots).toHaveLength(1);
    const parent = roots[0];
    expect(parent.node.key).toBe("-Users-me--sp-03bf9f55");
    expect(parent.children).toHaveLength(1);
    const vn = parent.children[0];
    expect(vn.node.name).toBe("translation-service");
    expect(vn.node.key).toMatch(/^__virtual:/);
    expect(vn.children).toHaveLength(1);
    expect(vn.children[0].node.key).toBe(
      "-Users-me--sp-03bf9f55-translation-service--claude-worktrees-main",
    );
  });

  it("virtual node aggregates sessionCount and hasOngoing from children", () => {
    const nodes = [
      { name: "proj", key: "-Users-me--proj", sessionCount: 1, hasOngoing: false },
      {
        name: "wt",
        key: "-Users-me--proj-svc--claude-worktrees-main",
        sessionCount: 2,
        hasOngoing: true,
      },
    ];
    const roots = buildTree(nodes);
    const vn = roots[0].children[0];
    expect(vn.node.sessionCount).toBe(2);
    expect(vn.node.hasOngoing).toBe(true);
  });
});

describe("detectWorktreeKind", () => {
  it("detects worktrees (old single-dash style)", () => {
    expect(detectWorktreeKind("-Users-me-backend", "-Users-me-backend-worktrees-EC-123")).toBe(
      "worktrees",
    );
  });

  it("detects claude-worktrees (-- style, last segment)", () => {
    expect(detectWorktreeKind("-Users-me-backend", "-Users-me-backend--claude-worktrees-fox")).toBe(
      "claude-worktrees",
    );
  });

  it("detects claude-worktrees when intermediate -- segment exists (virtual parent key)", () => {
    expect(
      detectWorktreeKind(
        "__virtual:-Users-me--proj:svc",
        "-Users-me--proj-svc--claude-worktrees-main",
      ),
    ).toBe("claude-worktrees");
  });

  it("returns null for non-worktree children", () => {
    expect(detectWorktreeKind("-Users-me-backend", "-Users-me-backend-tools")).toBeNull();
  });

  it("returns null for -- child that is not a worktree", () => {
    expect(detectWorktreeKind("-Users-me--proj", "-Users-me--proj--subproject")).toBeNull();
  });
});

describe("worktreeLeafName", () => {
  it("extracts leaf name for worktrees (old single-dash style)", () => {
    expect(
      worktreeLeafName("-Users-me-backend", "-Users-me-backend-worktrees-EC-123", "worktrees"),
    ).toBe("EC-123");
  });

  it("extracts leaf name for claude-worktrees (-- style)", () => {
    expect(
      worktreeLeafName(
        "-Users-me-backend",
        "-Users-me-backend--claude-worktrees-happy-crane",
        "claude-worktrees",
      ),
    ).toBe("happy-crane");
  });

  it("extracts leaf name via -- segment when virtual parent key is used", () => {
    expect(
      worktreeLeafName(
        "__virtual:-Users-me--proj:svc",
        "-Users-me--proj-svc--claude-worktrees-security-rubygems-addressable-133",
        "claude-worktrees",
      ),
    ).toBe("security-rubygems-addressable-133");
  });
});

describe("flattenTree", () => {
  it("flattens a simple tree", () => {
    const roots = buildTree([
      { name: "backend", key: "-Users-me-backend", sessionCount: 2, hasOngoing: true },
    ]);
    const flat = flattenTree(roots);
    expect(flat).toHaveLength(1);
    expect(omitExpandFields(flat)[0]).toEqual({
      key: "-Users-me-backend",
      name: "backend",
      count: 2,
      ongoing: true,
      depth: 0,
      isGroup: false,
    });
    expect(flat[0].hasChildren).toBe(false);
    expect(flat[0].isExpanded).toBe(true);
  });

  it("creates worktree group nodes", () => {
    const nodes = [
      { name: "backend", key: "-Users-me-backend", sessionCount: 1, hasOngoing: false },
      {
        name: "EC-123",
        key: "-Users-me-backend-worktrees-EC-123",
        sessionCount: 1,
        hasOngoing: false,
      },
    ];
    const roots = buildTree(nodes);
    const flat = flattenTree(roots);
    // parent, group header, leaf
    expect(flat).toHaveLength(3);
    expect(flat[0].hasChildren).toBe(true);
    expect(flat[0].isExpanded).toBe(true);
    expect(flat[1].isGroup).toBe(true);
    expect(flat[1].name).toBe("worktrees");
    expect(flat[1].hasChildren).toBe(true);
    expect(flat[2].name).toBe("EC-123");
    expect(flat[2].depth).toBe(2);
  });

  it("hides children of a collapsed project node", () => {
    const nodes = [
      { name: "backend", key: "-Users-me-backend", sessionCount: 1, hasOngoing: false },
      {
        name: "EC-123",
        key: "-Users-me-backend-worktrees-EC-123",
        sessionCount: 1,
        hasOngoing: false,
      },
    ];
    const collapsed = new Set(["-Users-me-backend"]);
    const flat = flattenTree(buildTree(nodes), collapsed);
    // only the parent; children hidden
    expect(flat).toHaveLength(1);
    expect(flat[0].isExpanded).toBe(false);
    expect(flat[0].hasChildren).toBe(true);
  });

  it("hides children of a collapsed group header", () => {
    const nodes = [
      { name: "backend", key: "-Users-me-backend", sessionCount: 1, hasOngoing: false },
      {
        name: "EC-123",
        key: "-Users-me-backend-worktrees-EC-123",
        sessionCount: 1,
        hasOngoing: false,
      },
    ];
    const collapsed = new Set(["__group:worktrees:-Users-me-backend"]);
    const flat = flattenTree(buildTree(nodes), collapsed);
    // parent + group header only; leaf hidden
    expect(flat).toHaveLength(2);
    expect(flat[1].isGroup).toBe(true);
    expect(flat[1].isExpanded).toBe(false);
  });

  it("shows virtual intermediate node between parent and worktree group", () => {
    const nodes = [
      { name: "sp-03bf9f55", key: "-Users-me--sp-03bf9f55", sessionCount: 1, hasOngoing: false },
      {
        name: "wt",
        key: "-Users-me--sp-03bf9f55-translation-service--claude-worktrees-security-rubygems-addressable-133",
        sessionCount: 1,
        hasOngoing: false,
      },
    ];
    const flat = flattenTree(buildTree(nodes));
    // depth 0: sp-03bf9f55
    // depth 1: translation-service (virtual)
    // depth 2: claude-worktrees (group)
    // depth 3: security-rubygems-addressable-133 (leaf)
    expect(flat).toHaveLength(4);
    expect(flat[0].name).toBe("sp-03bf9f55");
    expect(flat[0].depth).toBe(0);
    expect(flat[1].name).toBe("translation-service");
    expect(flat[1].depth).toBe(1);
    expect(flat[1].isGroup).toBe(false);
    expect(flat[2].name).toBe("claude-worktrees");
    expect(flat[2].isGroup).toBe(true);
    expect(flat[2].depth).toBe(2);
    expect(flat[3].name).toBe("security-rubygems-addressable-133");
    expect(flat[3].depth).toBe(3);
  });
});

describe("buildFlatItems", () => {
  it("prepends All Projects entry", () => {
    const sessions = [
      makeSession({ path: "/home/user/.claude/projects/proj-a/s1.jsonl", cwd: "/x/proj-a" }),
    ];
    const items = buildFlatItems(sessions);
    expect(omitExpandFields(items)[0]).toEqual({
      key: null,
      name: "All Projects",
      count: 1,
      ongoing: false,
      depth: 0,
      isGroup: false,
    });
    expect(items[0].hasChildren).toBe(false);
    expect(items[0].isExpanded).toBe(true);
  });

  it("returns correct total count in All Projects", () => {
    const sessions = [
      makeSession({ path: "/home/user/.claude/projects/proj-a/s1.jsonl", cwd: "/x/proj-a" }),
      makeSession({ path: "/home/user/.claude/projects/proj-a/s2.jsonl", cwd: "/x/proj-a" }),
      makeSession({ path: "/home/user/.claude/projects/proj-b/s3.jsonl", cwd: "/x/proj-b" }),
    ];
    const items = buildFlatItems(sessions);
    expect(items[0].count).toBe(3);
  });

  it("chains the full pipeline correctly", () => {
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
    const items = buildFlatItems(sessions);
    const keys = items.map((i) => i.key);
    expect(keys[0]).toBeNull(); // All Projects
    expect(keys[1]).toBe("-Users-me-backend");
    expect(keys[2]).toBe("__group:worktrees:-Users-me-backend");
    expect(keys[3]).toBe("-Users-me-backend-worktrees-EC-789");
  });
});
