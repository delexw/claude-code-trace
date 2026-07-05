import { useRef, useMemo } from "react";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { useVisibleSessions } from "../hooks/useVisibleSessions";
import type { SessionInfo } from "../types";
import { OngoingDots } from "./OngoingDots";
import {
  formatTokens,
  formatDuration,
  formatExactTime,
  formatCost,
  truncate,
  groupByDate,
  shortModel,
} from "../lib/format";
import { getModelColor } from "../lib/theme";
import { mergeRefs } from "../lib/mergeRefs";
import { BsClaude } from "react-icons/bs";
import { TokensIcon, CostIcon, ForwardIcon } from "./Icons";

interface SessionPickerProps {
  sessions: SessionInfo[];
  loading: boolean;
  searchQuery: string;
  selectedIndex: number;
  onSelect: (session: SessionInfo) => void;
  onSearchChange: (query: string) => void;
  onSelectIndex?: (index: number) => void;
  /**
   * Called (debounced) with the paths of session cards currently in the viewport,
   * and again periodically on a heartbeat while any cards remain visible. The caller
   * uses this as a cue to refresh fresh session info.
   */
  onVisiblePathsChange?: (paths: string[]) => void;
  /**
   * When true, sessions with an end-of-session recap and no user-assigned name
   * show the full recap (instead of the truncated name/first_message) as their preview.
   */
  recapPreview?: boolean;
}

export function SessionPicker({
  sessions,
  loading,
  searchQuery,
  selectedIndex,
  onSelect,
  onSearchChange,
  onSelectIndex,
  onVisiblePathsChange,
  recapPreview = false,
}: SessionPickerProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedIndex);
  const searchRef = useRef<HTMLInputElement>(null);
  const registerVisible = useVisibleSessions(onVisiblePathsChange ?? noop);

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
              <TokensIcon /> {formatTokens(totalTokens)} tok
            </span>
          )}
          {totalCost > 0 && (
            <span className="picker__total-cost">
              <CostIcon /> {formatCost(totalCost)}
            </span>
          )}
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
            <span className="braille-spinner" />
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
              const showRecap = recapPreview && !!session.recap;

              return (
                <div
                  key={session.path}
                  ref={mergeRefs(isSelected ? selectedRef : null, registerVisible(session.path))}
                  className={`picker__session${isSelected ? " picker__session--selected" : ""}${session.is_ongoing ? " picker__session--ongoing" : ""}`}
                  onMouseEnter={() => onSelectIndex?.(idx)}
                  onClick={() => onSelect(session)}
                >
                  <div className="picker__session-top">
                    <span className="picker__session-icon">
                      <BsClaude />
                    </span>
                    <span
                      className={`picker__session-preview${session.name ? " picker__session-preview--named" : ""}`}
                    >
                      {truncate(session.name || session.first_message || session.session_id, 80)}
                    </span>
                    {session.is_ongoing && (
                      <span className="picker__session-ongoing">
                        <OngoingDots count={1} />
                        ACTIVE
                      </span>
                    )}
                    <button
                      className="message__detail-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        onSelect(session);
                      }}
                    >
                      Detail <ForwardIcon />
                    </button>
                  </div>
                  {showRecap ? (
                    <div className="picker__session-subtitle picker__session-subtitle--recap">
                      <span className="picker__recap-label">Recap:</span> {session.recap}
                    </div>
                  ) : session.name && session.first_message ? (
                    <div className="picker__session-subtitle">
                      {truncate(session.first_message, 80)}
                    </div>
                  ) : null}
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
                        <CostIcon /> {formatCost(sessionCost)}
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

function noop() {}
