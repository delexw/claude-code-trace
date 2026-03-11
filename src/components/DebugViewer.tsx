import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import type { DebugEntry } from "../types";

type DebugLevel = "all" | "warn" | "error";

interface DebugViewerProps {
  entries: DebugEntry[];
  onBack: () => void;
}

export function DebugViewer({ entries, onBack }: DebugViewerProps) {
  const [levelFilter, setLevelFilter] = useState<DebugLevel>("all");
  const [searchText, setSearchText] = useState("");
  const [expandedSet, setExpandedSet] = useState<Set<number>>(new Set());
  const [selectedIndex, setSelectedIndex] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);

  // Filter entries
  const filtered = useMemo(() => {
    let result = entries;

    if (levelFilter === "warn") {
      result = result.filter(
        (e) => e.level === "warn" || e.level === "error"
      );
    } else if (levelFilter === "error") {
      result = result.filter((e) => e.level === "error");
    }

    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(
        (e) =>
          e.message.toLowerCase().includes(lower) ||
          e.category.toLowerCase().includes(lower) ||
          e.extra.toLowerCase().includes(lower)
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

  // Scroll into view
  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const toggleExpand = useCallback((index: number) => {
    setExpandedSet((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  // cycleLevelFilter is available for keyboard shortcut integration
  // const cycleLevelFilter = useCallback(() => { ... }, []);

  return (
    <div className="debug-viewer">
      <div className="debug-viewer__header">
        <button className="message-detail__back" onClick={onBack}>
          {"\u2190"} Back
        </button>
        <span className="debug-viewer__title">Debug Log</span>

        <div className="debug-viewer__filter-group">
          <button
            className={`debug-viewer__filter-btn${levelFilter === "all" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("all");
              setExpandedSet(new Set());
            }}
          >
            All
          </button>
          <button
            className={`debug-viewer__filter-btn${levelFilter === "warn" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("warn");
              setExpandedSet(new Set());
            }}
          >
            Warn+
          </button>
          <button
            className={`debug-viewer__filter-btn${levelFilter === "error" ? " debug-viewer__filter-btn--active" : ""}`}
            onClick={() => {
              setLevelFilter("error");
              setExpandedSet(new Set());
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
            setExpandedSet(new Set());
            setSelectedIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setSearchText("");
              setExpandedSet(new Set());
            }
          }}
        />

        <span className="debug-viewer__count">
          {filtered.length} / {entries.length}
        </span>
      </div>

      <div className="debug-viewer__body" ref={bodyRef}>
        {filtered.length === 0 && (
          <div className="picker__empty">No matching entries</div>
        )}

        {filtered.map((entry, idx) => {
          const isSelected = idx === selectedIndex;
          const isExpanded = expandedSet.has(idx);
          const hasExtra = !!entry.extra;

          return (
            <div key={`${entry.line_num}-${idx}`}>
              <div
                ref={isSelected ? selectedRef : undefined}
                className={`debug-entry${isSelected ? " debug-entry--selected" : ""}`}
                onClick={() => {
                  setSelectedIndex(idx);
                  if (hasExtra) toggleExpand(idx);
                }}
              >
                <span className="debug-entry__timestamp">
                  {entry.timestamp}
                </span>
                <span
                  className={`debug-entry__level debug-entry__level--${entry.level}`}
                >
                  {entry.level}
                </span>
                <span className="debug-entry__category">
                  {entry.category}
                </span>
                <span className="debug-entry__message">{entry.message}</span>
                {entry.count > 1 && (
                  <span className="debug-entry__count">
                    {"\u00D7"}{entry.count}
                  </span>
                )}
              </div>
              {isExpanded && hasExtra && (
                <div className="debug-entry__extra">{entry.extra}</div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
