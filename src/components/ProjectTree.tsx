import { useMemo } from "react";
import type { SessionInfo } from "../types";
import { shortPath, projectKey } from "../lib/format";

interface ProjectTreeProps {
  sessions: SessionInfo[];
  selectedProject: string | null;
  onSelectProject: (project: string | null) => void;
  onRefresh: () => void;
  refreshing?: boolean;
  style?: React.CSSProperties;
}

interface ProjectNode {
  name: string;
  key: string;
  sessionCount: number;
  hasOngoing: boolean;
}

export function ProjectTree({ sessions, selectedProject, onSelectProject, onRefresh, refreshing, style }: ProjectTreeProps) {
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

  return (
    <div className="project-tree" style={style}>
      <div className="project-tree__header">
        <span>Projects</span>
        <button
          className={`project-tree__refresh${refreshing ? " project-tree__refresh--spinning" : ""}`}
          onClick={(e) => { e.stopPropagation(); onRefresh(); }}
          title="Refresh all projects"
        >
          {"\u21BB"}
        </button>
      </div>
      <div className="project-tree__list">
        <div
          className={`project-tree__item${selectedProject === null ? " project-tree__item--selected" : ""}`}
          onClick={() => onSelectProject(null)}
        >
          <span className="project-tree__name">All Projects</span>
          <span className="project-tree__count">{sessions.length}</span>
        </div>
        {projects.map((p) => (
          <div
            key={p.key}
            className={`project-tree__item${selectedProject === p.key ? " project-tree__item--selected" : ""}`}
            onClick={() => onSelectProject(p.key)}
          >
            <span className="project-tree__name" title={p.key}>
              {p.name}
            </span>
            <span className="project-tree__meta">
              {p.hasOngoing && <span className="project-tree__ongoing-dot" />}
              <span className="project-tree__count">{p.sessionCount}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
