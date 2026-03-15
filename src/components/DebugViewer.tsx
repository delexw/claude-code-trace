import { useState, useRef, useEffect, useMemo } from "react";
import type { DebugEntry } from "../types";
import { useToggleSet } from "../hooks/useToggleSet";
import { useScrollToSelected } from "../hooks/useScrollToSelected";

type DebugLevel = "all" | "warn" | "error";

interface DebugViewerProps {
  entries: DebugEntry[];
}

export function DebugViewer({ entries }: DebugViewerProps) {
  const [levelFilter, setLevelFilter] = useState<DebugLevel>("all");
  const [searchText, setSearchText] = useState("");
  const { set: expandedSet, toggle: toggleExpand, clear: clearExpanded } = useToggleSet();
  const [selectedIndex, setSelectedIndex] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedIndex);

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

  // Clamp selected index
  useEffect(() => {
    if (selectedIndex >= filtered.length && filtered.length > 0) {
      setSelectedIndex(filtered.length - 1);
    }
  }, [filtered.length, selectedIndex]);

  // cycleLevelFilter is available for keyboard shortcut integration
  // const cycleLevelFilter = useCallback(() => { ... }, []);

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
