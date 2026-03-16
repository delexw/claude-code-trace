import { useMemo } from "react";
import type { SessionInfo } from "../types";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { RefreshIcon, GitBranchIcon, GitMergeIcon } from "./Icons";
import { OngoingDots } from "./OngoingDots";
import { buildFlatItems } from "../../shared/projectTree";

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

/**
 * Returns the ordered list of selectable keys as displayed in the tree.
 * Index 0 = null ("All Projects"), then project keys and group keys in tree order.
 */
export function useProjectKeys(sessions: SessionInfo[]): (string | null)[] {
  return useMemo(() => {
    const flat = buildFlatItems(sessions);
    return flat.map((f) => f.key);
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
  const allItems = useMemo(() => buildFlatItems(sessions), [sessions]);

  const scrollRef = useScrollToSelected(highlightedIndex);

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
