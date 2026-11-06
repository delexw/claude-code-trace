import { useCallback, useMemo, useState } from "react";
import { ProjectTree } from "../../components/ProjectTree";
import type { SourceSidebar } from "../types";
import { buildCodexProjectItems } from "./projects";
import type { CodexSession } from "./types";

interface Options {
  sessions: CodexSession[];
  loading: boolean;
  onRefresh(): void;
  onSelect(): void;
}

/** Source-owned project selection survives opening chats, searching and refreshes. */
export function useCodexProjects({ sessions, loading, onRefresh, onSelect }: Options): {
  selectedProject: string | null;
  sidebar: SourceSidebar;
} {
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [focused, setFocused] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(() => new Set());
  const items = useMemo(
    () => buildCodexProjectItems(sessions, collapsedKeys),
    [sessions, collapsedKeys],
  );
  const activeIndex = Math.min(highlightedIndex, Math.max(0, items.length - 1));
  const select = useCallback(
    (key: string | null) => {
      setSelectedProject(key);
      setFocused(false);
      onSelect();
    },
    [onSelect],
  );
  const toggleCollapse = useCallback((key: string) => {
    setCollapsedKeys((before) => {
      const next = new Set(before);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
  const down = () => setHighlightedIndex(Math.min(activeIndex + 1, items.length - 1));
  const up = () => setHighlightedIndex(Math.max(activeIndex - 1, 0));
  const current = items[activeIndex];
  const blur = () => setFocused(false);
  const toggle = () => {
    if (current?.hasChildren && current.key) toggleCollapse(current.key);
  };
  return {
    selectedProject,
    sidebar: {
      focused,
      focus: () => setFocused(true),
      blur,
      keyMap: {
        j: down,
        ArrowDown: down,
        k: up,
        ArrowUp: up,
        Enter: () => {
          if (current && !current.isGroup) select(current.key);
        },
        Escape: blur,
        l: blur,
        " ": toggle,
        ArrowRight: () => {
          if (current?.hasChildren && !current.isExpanded) toggle();
          else blur();
        },
        ArrowLeft: () => {
          if (current?.hasChildren && current.isExpanded) toggle();
        },
      },
      render: (style) => (
        <ProjectTree
          items={items}
          heading="Codex Projects"
          selectedProject={selectedProject}
          highlightedIndex={activeIndex}
          isFocused={focused}
          onSelectProject={select}
          onToggleCollapse={toggleCollapse}
          onRefresh={onRefresh}
          onFocus={() => setFocused(true)}
          refreshing={loading}
          style={style}
        />
      ),
    },
  };
}
