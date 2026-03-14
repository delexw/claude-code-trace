import { useMemo } from "react";
import type { SessionInfo } from "../types";
import { shortPath, projectKey, projectDisplayName } from "../lib/format";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { RefreshIcon, GitBranchIcon, GitMergeIcon } from "./Icons";
import { OngoingDots } from "./OngoingDots";

interface ProjectTreeProps {
  sessions: SessionInfo[];
  selectedProject: string | null;
  highlightedIndex?: number;
  isFocused?: boolean;
  onSelectProject: (project: string | null) => void;
  onRefresh: () => void;
  onFocus?: () => void;
  refreshing?: boolean;
  style?: React.CSSProperties;
}

class ProjectNode {
  constructor(
    public name: string,
    public key: string,
    public sessionCount: number,
    public hasOngoing: boolean,
  ) {}

  addSession(isOngoing: boolean) {
    this.sessionCount++;
    if (isOngoing) this.hasOngoing = true;
  }

  static fromSessions(sessions: SessionInfo[]): ProjectNode[] {
    const map = new Map<string, ProjectNode>();
    for (const s of sessions) {
      const key = projectKey(s.path);
      const existing = map.get(key);
      if (existing) {
        existing.addSession(s.is_ongoing);
      } else {
        map.set(
          key,
          new ProjectNode(shortPath(s.cwd) || projectDisplayName(key), key, 1, s.is_ongoing),
        );
      }
    }
    return [...map.values()].toSorted((a, b) => a.name.localeCompare(b.name));
  }
}

class TreeNode {
  children: TreeNode[] = [];

  constructor(public node: ProjectNode) {}

  static cmp(a: TreeNode, b: TreeNode): number {
    return a.node.name.localeCompare(b.node.name);
  }

  /** Short label relative to a parent key. */
  static childLabel(parentKey: string, childKey: string): string {
    return childKey.slice(parentKey.length).replace(/^-+/, "") || projectDisplayName(childKey);
  }

  /** Build a tree from flat nodes — child if B.key starts with A.key + "-". */
  static buildTree(nodes: ProjectNode[]): TreeNode[] {
    const sorted = nodes.toSorted((a, b) => a.key.length - b.key.length);
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

      const tn = new TreeNode(node);
      all.push(tn);

      if (parent) {
        tn.node = new ProjectNode(
          TreeNode.childLabel(parent.node.key, node.key),
          node.key,
          node.sessionCount,
          node.hasOngoing,
        );
        parent.children.push(tn);
      } else {
        roots.push(tn);
      }
    }

    roots.sort(TreeNode.cmp);
    for (const r of all) r.children.sort(TreeNode.cmp);
    return roots;
  }
}

type WorktreeKind = "worktrees" | "claude-worktrees";

class FlatItem {
  constructor(
    public key: string | null,
    public name: string,
    public count: number,
    public ongoing: boolean,
    public depth: number,
    public isGroup: boolean,
  ) {}

  static detectWorktreeKind(parentKey: string, childKey: string): WorktreeKind | null {
    const rest = childKey.slice(parentKey.length);
    if (rest.startsWith("--claude-worktrees-")) return "claude-worktrees";
    if (rest.startsWith("-worktrees-")) return "worktrees";
    return null;
  }

  static worktreeLeafName(parentKey: string, childKey: string, kind: WorktreeKind): string {
    const prefixLen =
      kind === "claude-worktrees" ? "--claude-worktrees-".length : "-worktrees-".length;
    return (
      childKey.slice(parentKey.length + prefixLen).replace(/^-+/, "") ||
      projectDisplayName(childKey)
    );
  }

  static fromTree(roots: TreeNode[]): FlatItem[] {
    const items: FlatItem[] = [];

    function walk(nodes: TreeNode[], depth: number, parentKey: string | null) {
      for (const tn of nodes) {
        items.push(
          new FlatItem(
            tn.node.key,
            tn.node.name,
            tn.node.sessionCount,
            tn.node.hasOngoing,
            depth,
            false,
          ),
        );

        // Categorise children into worktree groups and regular
        const groups = new Map<WorktreeKind, TreeNode[]>();
        const regular: TreeNode[] = [];
        for (const child of tn.children) {
          const kind =
            parentKey !== null || depth === 0
              ? FlatItem.detectWorktreeKind(tn.node.key, child.node.key)
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
          items.push(
            new FlatItem(
              `__group:${kind}:${tn.node.key}`,
              kind,
              totalCount,
              anyOngoing,
              depth + 1,
              true,
            ),
          );
          for (const child of children) {
            items.push(
              new FlatItem(
                child.node.key,
                FlatItem.worktreeLeafName(tn.node.key, child.node.key, kind),
                child.node.sessionCount,
                child.node.hasOngoing,
                depth + 2,
                false,
              ),
            );
            walk(child.children, depth + 3, child.node.key);
          }
        }

        walk(regular, depth + 1, tn.node.key);
      }
    }

    walk(roots, 0, null);
    return items;
  }
}

/**
 * Returns the ordered list of selectable keys as displayed in the tree.
 * Index 0 = null ("All Projects"), then project keys and group keys in tree order.
 */
export function useProjectKeys(sessions: SessionInfo[]): (string | null)[] {
  return useMemo(() => {
    const nodes = ProjectNode.fromSessions(sessions);
    const tree = TreeNode.buildTree(nodes);
    const flat = FlatItem.fromTree(tree);
    return [null, ...flat.map((f) => f.key)];
  }, [sessions]);
}

export function ProjectTree({
  sessions,
  selectedProject,
  highlightedIndex = 0,
  isFocused = false,
  onSelectProject,
  onRefresh,
  onFocus,
  refreshing,
  style,
}: ProjectTreeProps) {
  const flatItems = useMemo(() => {
    const nodes = ProjectNode.fromSessions(sessions);
    const tree = TreeNode.buildTree(nodes);
    return FlatItem.fromTree(tree);
  }, [sessions]);

  const scrollRef = useScrollToSelected(highlightedIndex);

  const allItems: FlatItem[] = [
    {
      key: null,
      name: "All Projects",
      count: sessions.length,
      ongoing: false,
      depth: 0,
      isGroup: false,
    },
    ...flatItems,
  ];

  return (
    <div
      className={`project-tree${isFocused ? " project-tree--focused" : ""}`}
      style={style}
      onClick={onFocus}
    >
      <div className="project-tree__header">
        <span>Projects</span>
        <button
          className={`project-tree__refresh${refreshing ? " project-tree__refresh--spinning" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            onRefresh();
          }}
          title="Refresh all projects"
        >
          <RefreshIcon />
        </button>
      </div>
      <div className="project-tree__list">
        {allItems.map((item, idx) => {
          const isSelected = !item.isGroup && selectedProject === item.key;
          const isHighlighted = isFocused && idx === highlightedIndex;
          return (
            <div
              key={item.key ?? "__all__"}
              ref={isHighlighted ? scrollRef : undefined}
              className={`project-tree__item${isSelected ? " project-tree__item--selected" : ""}${isHighlighted ? " project-tree__item--highlighted" : ""}${item.depth > 0 ? " project-tree__item--child" : ""}${item.isGroup ? " project-tree__item--group" : ""}`}
              style={item.depth > 0 ? { paddingLeft: 12 + item.depth * 16 } : undefined}
              onClick={item.isGroup ? undefined : () => onSelectProject(item.key)}
            >
              <span
                className="project-tree__name"
                title={item.isGroup ? undefined : (item.key ?? undefined)}
              >
                {item.depth > 0 && <span className="project-tree__branch">{"\u2514"} </span>}
                {item.isGroup && (
                  <span className="project-tree__group-icon">
                    {item.name === "worktrees" ? <GitBranchIcon /> : <GitMergeIcon />}
                  </span>
                )}
                {item.name}
              </span>
              <span className="project-tree__meta">
                {item.ongoing && <OngoingDots count={1} />}
                <span className="project-tree__count">{item.count}</span>
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
