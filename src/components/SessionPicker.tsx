import { useRef, useMemo } from "react";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import type { SessionInfo } from "../types";
import {
  formatTokens,
  formatDuration,
  formatExactTime,
  formatCost,
  truncate,
  groupByDate,
  shortModel,
} from "../lib/format";
import { getModelColor, spinnerFrames } from "../lib/theme";

interface SessionPickerProps {
  sessions: SessionInfo[];
  loading: boolean;
  searchQuery: string;
  selectedIndex: number;
  onSelect: (session: SessionInfo) => void;
  onSearchChange: (query: string) => void;
  onSelectIndex?: (index: number) => void;
  animFrame: number;
}

export function SessionPicker({
  sessions,
  loading,
  searchQuery,
  selectedIndex,
  onSelect,
  onSearchChange,
  onSelectIndex,
  animFrame,
}: SessionPickerProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedIndex);
  const searchRef = useRef<HTMLInputElement>(null);

  const dateGroups = groupByDate(sessions);

  const { totalTokens, totalCost } = useMemo(() => {
    let tokens = 0;
    let cost = 0;
    for (const s of sessions) {
      tokens += s.total_tokens;
      cost += s.cost_usd;
    }
    return { totalTokens: tokens, totalCost: cost };
  }, [sessions]);

  // Build flat list for index tracking
  let flatIndex = 0;

  return (
    <div className="picker">
      <div className="picker__header">
        <div className="picker__title">
          Sessions
          {totalTokens > 0 && (
            <span className="picker__total-tokens">
              {"\u{1FA99}"} {formatTokens(totalTokens)} tok
            </span>
          )}
          {totalCost > 0 && <span className="picker__total-cost">{formatCost(totalCost)}</span>}
        </div>
        <input
          ref={searchRef}
          className="picker__search"
          type="text"
          placeholder="Search sessions..."
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>

      <div className="picker__list" ref={listRef}>
        {loading && (
          <div className="picker__loading">
            <span className="spinner">{spinnerFrames[animFrame % spinnerFrames.length]}</span>
            Discovering sessions...
          </div>
        )}

        {!loading && sessions.length === 0 && (
          <div className="picker__empty">
            {searchQuery ? "No matching sessions" : "No sessions found"}
          </div>
        )}

        {dateGroups.map((group) => (
          <div key={group.category}>
            <div className="picker__group-header">{group.category}</div>
            {group.items.map((session) => {
              const idx = flatIndex++;
              const isSelected = idx === selectedIndex;
              const model = shortModel(session.model);
              const modelClr = getModelColor(session.model);
              const sessionCost = session.cost_usd;

              return (
                <div
                  key={session.path}
                  ref={isSelected ? selectedRef : undefined}
                  className={`picker__session${isSelected ? " picker__session--selected" : ""}${session.is_ongoing ? " picker__session--ongoing" : ""}`}
                  onMouseEnter={() => onSelectIndex?.(idx)}
                  onClick={() => onSelect(session)}
                >
                  <div className="picker__session-top">
                    <span className="picker__session-preview">
                      {truncate(session.first_message || session.session_id, 80)}
                    </span>
                    {session.is_ongoing && (
                      <span className="picker__session-ongoing">
                        <span className="picker__session-ongoing-dot" />
                        ACTIVE
                      </span>
                    )}
                  </div>
                  <div className="picker__session-meta">
                    <span className="picker__session-model" style={{ color: modelClr }}>
                      {model}
                    </span>
                    <span className="picker__session-stat">{session.turn_count} turns</span>
                    {session.total_tokens > 0 && (
                      <span className="picker__session-stat">
                        {formatTokens(session.total_tokens)} tok
                      </span>
                    )}
                    {sessionCost > 0 && (
                      <span className="picker__session-stat picker__session-stat--cost">
                        {formatCost(sessionCost)}
                      </span>
                    )}
                    {session.duration_ms > 0 && (
                      <span className="picker__session-stat">
                        {formatDuration(session.duration_ms)}
                      </span>
                    )}
                    {session.mod_time && (
                      <span className="picker__session-time">
                        {formatExactTime(session.mod_time)}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
