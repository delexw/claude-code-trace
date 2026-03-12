import { useMemo } from "react";
import type { SessionInfo } from "../types";
import { shortPath, projectKey } from "../lib/format";
import { useScrollToSelected } from "../hooks/useScrollToSelected";

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

interface ProjectNode {
  name: string;
  key: string;
  sessionCount: number;
  hasOngoing: boolean;
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
  const projects = useMemo(() => {
    const map = new Map<string, { name: string; count: number; ongoing: boolean }>();

    for (const s of sessions) {
      const key = projectKey(s.path);
      const existing = map.get(key);
      if (existing) {
        existing.count++;
        if (s.is_ongoing) existing.ongoing = true;
      } else {
        map.set(key, {
          name: shortPath(s.cwd) || key,
          count: 1,
          ongoing: s.is_ongoing,
        });
      }
    }

    const nodes: ProjectNode[] = [];
    for (const [key, val] of map) {
      nodes.push({
        name: val.name,
        key,
        sessionCount: val.count,
        hasOngoing: val.ongoing,
      });
    }

    nodes.sort((a, b) => a.name.localeCompare(b.name));
    return nodes;
  }, [sessions]);

  const scrollRef = useScrollToSelected(highlightedIndex);

  // Flat list: index 0 = "All Projects", index 1+ = project nodes
  const allItems: { key: string | null; name: string; count: number; ongoing: boolean }[] = [
    { key: null, name: "All Projects", count: sessions.length, ongoing: false },
    ...projects.map((p) => ({
      key: p.key,
      name: p.name,
      count: p.sessionCount,
      ongoing: p.hasOngoing,
    })),
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
          {"\u21BB"}
        </button>
      </div>
      <div className="project-tree__list">
        {allItems.map((item, idx) => {
          const isSelected = selectedProject === item.key;
          const isHighlighted = isFocused && idx === highlightedIndex;
          return (
            <div
              key={item.key ?? "__all__"}
              ref={isHighlighted ? scrollRef : undefined}
              className={`project-tree__item${isSelected ? " project-tree__item--selected" : ""}${isHighlighted ? " project-tree__item--highlighted" : ""}`}
              onClick={() => onSelectProject(item.key)}
            >
              <span className="project-tree__name" title={item.key ?? undefined}>
                {item.name}
              </span>
              <span className="project-tree__meta">
                {item.ongoing && <span className="project-tree__ongoing-dot" />}
                <span className="project-tree__count">{item.count}</span>
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Returns the total number of items in the project tree (1 for "All" + projects) */
export function useProjectCount(sessions: SessionInfo[]): number {
  const keys = new Set<string>();
  for (const s of sessions) {
    keys.add(projectKey(s.path));
  }
  return keys.size + 1; // +1 for "All Projects"
}
