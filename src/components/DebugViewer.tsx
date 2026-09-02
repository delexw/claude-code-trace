import { useState, useRef, useMemo, useCallback } from "react";
import type { DebugEntry } from "../types";
import { useToggleSet } from "../hooks/useToggleSet";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";

type DebugLevel = "all" | "warn" | "error";

interface DebugViewerProps {
  entries: DebugEntry[];
  viewActionsRef: ViewActionsRef;
}

export function DebugViewer({ entries, viewActionsRef }: DebugViewerProps) {
  const [levelFilter, setLevelFilter] = useState<DebugLevel>("all");
  const [searchText, setSearchText] = useState("");
  const { set: expandedSet, toggle: toggleExpand, clear: clearExpanded, addAll } = useToggleSet();
  const [storedSelectedIndex, setSelectedIndex] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);

  // Filter entries
  const filtered = useMemo(() => {
    let result = entries;

    if (levelFilter === "warn") {
      result = result.filter((e) => e.level === "warn" || e.level === "error");
    } else if (levelFilter === "error") {
      result = result.filter((e) => e.level === "error");
    }

    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(
        (e) =>
          e.message.toLowerCase().includes(lower) ||
          e.category.toLowerCase().includes(lower) ||
          e.extra.toLowerCase().includes(lower),
      );
    }

    return result;
  }, [entries, levelFilter, searchText]);

  // Clamp during render rather than from an effect: filtering can shrink the list below the
  // stored index, and correcting that in an effect renders once with an out-of-range index
  // (scrolling to a row that no longer exists) before fixing it up.
  const selectedIndex =
    filtered.length > 0 ? Math.min(storedSelectedIndex, filtered.length - 1) : storedSelectedIndex;
  const selectedRef = useScrollToSelected(selectedIndex);

  const debugExpandAll = useCallback(() => {
    const indices = filtered.map((_, i) => i).filter((i) => !!filtered[i].extra);
    addAll(indices);
  }, [filtered, addAll]);

  useRegisterViewActions(viewActionsRef, { expandAll: debugExpandAll, collapseAll: clearExpanded });

  return (
    <div className="debug-viewer">
      <div className="debug-viewer__header">
        <span className="debug-viewer__title">Debug Log</span>

        <div className="debug-viewer__filter-group">
          <button
            className={`debug-viewer__filter-btn${levelFilter === "all" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("all");
              clearExpanded();
            }}
          >
            All
          </button>
          <button
            className={`debug-viewer__filter-btn${levelFilter === "warn" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("warn");
              clearExpanded();
            }}
          >
            Warn+
          </button>
          <button
            className={`debug-viewer__filter-btn${levelFilter === "error" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("error");
              clearExpanded();
            }}
          >
            Error
          </button>
        </div>

        <input
          className="debug-viewer__search"
          type="text"
          placeholder="Filter text..."
          value={searchText}
          onChange={(e) => {
            setSearchText(e.target.value);
            clearExpanded();
            setSelectedIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setSearchText("");
              clearExpanded();
            }
          }}
        />

        <span className="debug-viewer__count">
          {filtered.length} / {entries.length}
        </span>
      </div>

      <div className="debug-viewer__body" ref={bodyRef}>
        {filtered.length === 0 && <div className="picker__empty">No matching entries</div>}

        {filtered.map((entry, idx) => {
          const isSelected = idx === selectedIndex;
          const isExpanded = expandedSet.has(idx);
          const hasExtra = !!entry.extra;

          return (
            <div key={entry.line_num}>
              <div
                ref={isSelected ? selectedRef : null}
                className={`debug-entry${isSelected ? " debug-entry--selected" : ""}`}
                onClick={() => {
                  setSelectedIndex(idx);
                  if (hasExtra) toggleExpand(idx);
                }}
              >
                <span className="debug-entry__timestamp">{entry.timestamp}</span>
                <span className={`debug-entry__level debug-entry__level--${entry.level}`}>
                  {entry.level}
                </span>
                <span className="debug-entry__category">{entry.category}</span>
                <span className="debug-entry__message">{entry.message}</span>
                {entry.count > 1 && (
                  <span className="debug-entry__count">
                    {"\u00D7"}
                    {entry.count}
                  </span>
                )}
              </div>
              {isExpanded && hasExtra && <div className="debug-entry__extra">{entry.extra}</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
}
