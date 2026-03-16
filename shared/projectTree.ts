/**
 * Pure tree-building logic for the project sidebar.
 * Shared between the web UI and TUI — no React, no DOM.
 */

import type { SessionInfo } from "./types.ts";
import { projectKey, projectDisplayName, shortPath } from "./format.ts";

// ---- ProjectNode ----

export interface ProjectNode {
  name: string;
  key: string;
  sessionCount: number;
  hasOngoing: boolean;
}

export function buildProjectNodes(sessions: SessionInfo[]): ProjectNode[] {
  const map = new Map<string, ProjectNode>();
  for (const s of sessions) {
    const key = projectKey(s.path);
    const existing = map.get(key);
    if (existing) {
      existing.sessionCount++;
      if (s.is_ongoing) existing.hasOngoing = true;
    } else {
      map.set(key, {
        name: shortPath(s.cwd) || projectDisplayName(key),
        key,
        sessionCount: 1,
        hasOngoing: s.is_ongoing,
      });
    }
  }
  return [...map.values()].toSorted((a, b) => a.name.localeCompare(b.name));
}

// ---- TreeNode ----

export interface TreeNode {
  node: ProjectNode;
  children: TreeNode[];
}

export function treeNodeCmp(a: TreeNode, b: TreeNode): number {
  return a.node.name.localeCompare(b.node.name);
}

export function buildTree(nodes: ProjectNode[]): TreeNode[] {
  const sorted = [...nodes].toSorted((a, b) => a.key.length - b.key.length);
  const roots: TreeNode[] = [];
  const all: TreeNode[] = [];

  for (const node of sorted) {
    let parent: TreeNode | undefined;
    for (const candidate of all) {
      if (
        node.key.startsWith(candidate.node.key + "-") &&
        (!parent || candidate.node.key.length > parent.node.key.length)
      ) {
        parent = candidate;
      }
    }

    const tn: TreeNode = { node, children: [] };
    all.push(tn);

    if (parent) {
      const childLabel =
        node.key.slice(parent.node.key.length).replace(/^-+/, "") || projectDisplayName(node.key);
      tn.node = { ...node, name: node.name || childLabel };
      parent.children.push(tn);
    } else {
      roots.push(tn);
    }
  }

  roots.sort(treeNodeCmp);
  for (const r of all) r.children.sort(treeNodeCmp);
  return roots;
}

// ---- Worktree helpers ----

export type WorktreeKind = "worktrees" | "claude-worktrees";

export function detectWorktreeKind(parentKey: string, childKey: string): WorktreeKind | null {
  const rest = childKey.slice(parentKey.length);
  if (rest.startsWith("--claude-worktrees-")) return "claude-worktrees";
  if (rest.startsWith("-worktrees-")) return "worktrees";
  return null;
}

export function worktreeLeafName(parentKey: string, childKey: string, kind: WorktreeKind): string {
  const prefixLen =
    kind === "claude-worktrees" ? "--claude-worktrees-".length : "-worktrees-".length;
  return (
    childKey.slice(parentKey.length + prefixLen).replace(/^-+/, "") || projectDisplayName(childKey)
  );
}

// ---- FlatItem ----

export interface FlatItem {
  key: string | null;
  name: string;
  count: number;
  ongoing: boolean;
  depth: number;
  isGroup: boolean;
}

export function flattenTree(roots: TreeNode[]): FlatItem[] {
  const items: FlatItem[] = [];

  function walk(nodes: TreeNode[], depth: number, parentKey: string | null) {
    for (const tn of nodes) {
      items.push({
        key: tn.node.key,
        name: tn.node.name,
        count: tn.node.sessionCount,
        ongoing: tn.node.hasOngoing,
        depth,
        isGroup: false,
      });

      // Categorise children into worktree groups and regular
      const groups = new Map<WorktreeKind, TreeNode[]>();
      const regular: TreeNode[] = [];
      for (const child of tn.children) {
        const kind =
          parentKey !== null || depth === 0
            ? detectWorktreeKind(tn.node.key, child.node.key)
            : null;
        if (kind) {
          let list = groups.get(kind);
          if (!list) {
            list = [];
            groups.set(kind, list);
          }
          list.push(child);
        } else {
          regular.push(child);
        }
      }

      // Emit worktree group headers + their children
      for (const [kind, children] of groups) {
        const totalCount = children.reduce((s, c) => s + c.node.sessionCount, 0);
        const anyOngoing = children.some((c) => c.node.hasOngoing);
        items.push({
          key: `__group:${kind}:${tn.node.key}`,
          name: kind,
          count: totalCount,
          ongoing: anyOngoing,
          depth: depth + 1,
          isGroup: true,
        });
        for (const child of children) {
          items.push({
            key: child.node.key,
            name: worktreeLeafName(tn.node.key, child.node.key, kind),
            count: child.node.sessionCount,
            ongoing: child.node.hasOngoing,
            depth: depth + 2,
            isGroup: false,
          });
          walk(child.children, depth + 3, child.node.key);
        }
      }

      walk(regular, depth + 1, tn.node.key);
    }
  }

  walk(roots, 0, null);
  return items;
}

// ---- Convenience ----

/** Chains buildProjectNodes -> buildTree -> flattenTree and prepends "All Projects". */
export function buildFlatItems(sessions: SessionInfo[]): FlatItem[] {
  const nodes = buildProjectNodes(sessions);
  const tree = buildTree(nodes);
  const flat = flattenTree(tree);
  return [
    {
      key: null,
      name: "All Projects",
      count: sessions.length,
      ongoing: false,
      depth: 0,
      isGroup: false,
    },
    ...flat,
  ];
}
