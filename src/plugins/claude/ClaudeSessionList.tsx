import { useMemo } from "react";
import { BsClaude } from "react-icons/bs";
import { useScrollToSelected } from "../../hooks/useScrollToSelected";
import { useVisibleSessions } from "../../hooks/useVisibleSessions";
import type {
  EfficiencyAnalysisJob,
  EfficiencySummary,
  IndexProgress,
  SessionInfo,
} from "../../types";
import { OngoingDots } from "../../components/OngoingDots";
import { TokensIcon, CostIcon, ForwardIcon } from "../../components/Icons";
import { BetaBadge } from "../../components/BetaBadge";
import {
  formatTokens,
  formatDuration,
  formatExactTime,
  formatCost,
  truncate,
  groupByDate,
  shortModel,
  shortPath,
} from "../../lib/format";
import { getModelColor } from "../../lib/theme";
import { mergeRefs } from "../../lib/mergeRefs";

const NO_EFFICIENCY_JOBS = new Map<string, EfficiencyAnalysisJob>();
const NO_EFFICIENCY_SUMMARIES = new Map<string, EfficiencySummary>();

function SessionAnalysisProgress({ progress }: { progress: number }) {
  return (
    <div
      className="picker__analysis-progress"
      role="progressbar"
      aria-label="Jev analysis progress"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={progress}
    >
      <div className="picker__analysis-progress-track">
        <span style={{ width: `${progress}%` }} />
      </div>
      <span className="picker__analysis-progress-value">{progress}%</span>
    </div>
  );
}

/** Total tokens and cost across the listed sessions, for the picker header. */
export function ClaudeHeaderStats({ sessions }: { sessions: SessionInfo[] }) {
  const { totalTokens, totalCost } = useMemo(() => {
    let tokens = 0;
    let cost = 0;
    for (const s of sessions) {
      tokens += s.total_tokens;
      cost += s.cost_usd;
    }
    return { totalTokens: tokens, totalCost: cost };
  }, [sessions]);

  return (
    <>
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
    </>
  );
}

export interface ClaudeSessionListProps {
  sessions: SessionInfo[];
  /** How far the backend has got reading the project directories. */
  index: IndexProgress;
  loading: boolean;
  searchQuery: string;
  /** Index within this list, or null when the selection is elsewhere. */
  selectedIndex: number | null;
  onSelect: (session: SessionInfo) => void;
  onHover?: (index: number) => void;
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
  efficiencyJobs?: Map<string, EfficiencyAnalysisJob>;
  efficiencySummaries?: Map<string, EfficiencySummary>;
  onAnalyse?: (path: string) => void;
  onOpenEfficiencyDashboard?: (session: SessionInfo) => void;
}

export function ClaudeSessionList({
  sessions,
  index,
  loading,
  searchQuery,
  selectedIndex,
  onSelect,
  onHover,
  onVisiblePathsChange,
  recapPreview = false,
  efficiencyJobs = NO_EFFICIENCY_JOBS,
  efficiencySummaries = NO_EFFICIENCY_SUMMARIES,
  onAnalyse,
  onOpenEfficiencyDashboard,
}: ClaudeSessionListProps) {
  const selectedRef = useScrollToSelected(selectedIndex ?? -1);
  const registerVisible = useVisibleSessions(onVisiblePathsChange ?? noop);

  const dateGroups = groupByDate(sessions);

  // Build flat list for index tracking
  let flatIndex = 0;

  return (
    <>
      {loading && (
        <div className="picker__loading">
          <span className="braille-spinner" />
          Discovering sessions...
        </div>
      )}

      {!loading && sessions.length === 0 && index.done && (
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
            const efficiencyJob = efficiencyJobs.get(session.path);
            const efficiencySummary = efficiencySummaries.get(session.path);
            const analysisRunning =
              efficiencyJob && !["completed", "failed", "cancelled"].includes(efficiencyJob.status);
            const analysisProgress = efficiencyJob?.progress ?? 0;
            const efficiencyTone = efficiencySummary
              ? efficiencySummary.score >= 87
                ? "ok"
                : efficiencySummary.score >= 65
                  ? "accent"
                  : efficiencySummary.score >= 40
                    ? "warn"
                    : "critical"
              : null;

            return (
              <div
                key={session.path}
                ref={mergeRefs(isSelected ? selectedRef : null, registerVisible(session.path))}
                className={`picker__session${isSelected ? " picker__session--selected" : ""}${session.is_ongoing ? " picker__session--ongoing" : ""}`}
                onMouseEnter={() => onHover?.(idx)}
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
                  {efficiencySummary && (
                    <>
                      <button
                        type="button"
                        className={`picker__efficiency-score picker__efficiency-score--${efficiencyTone}`}
                        title={`Open efficiency dashboard with score ${efficiencySummary.score}`}
                        onClick={(event) => {
                          event.stopPropagation();
                          onOpenEfficiencyDashboard?.(session);
                        }}
                      >
                        Dashboard · {efficiencySummary.score}
                        {efficiencySummary.stale ? " · stale" : ""}
                      </button>
                      {analysisRunning ? (
                        <SessionAnalysisProgress progress={analysisProgress} />
                      ) : onAnalyse ? (
                        <button
                          type="button"
                          className="picker__analyse"
                          onClick={(event) => {
                            event.stopPropagation();
                            onAnalyse(session.path);
                          }}
                        >
                          Re-analyse <BetaBadge />
                        </button>
                      ) : null}
                    </>
                  )}
                  {!efficiencySummary && analysisRunning && (
                    <SessionAnalysisProgress progress={analysisProgress} />
                  )}
                  {!efficiencySummary && !analysisRunning && onAnalyse && (
                    <button
                      type="button"
                      className="picker__analyse"
                      onClick={(event) => {
                        event.stopPropagation();
                        onAnalyse(session.path);
                      }}
                    >
                      {efficiencyJob?.status === "failed" ? "Retry analysis" : "Analyse"}{" "}
                      <BetaBadge />
                    </button>
                  )}
                  {efficiencyJob?.status === "failed" && (
                    <span
                      className="picker__analysis-error"
                      role="status"
                      title={efficiencyJob.error || efficiencyJob.message}
                    >
                      {efficiencyJob.error || efficiencyJob.message}
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
                {session.dirs && session.dirs.length > 1 && (
                  <div className="picker__session-dirs">
                    <span className="picker__session-dirs-label">CWDs:</span>{" "}
                    {session.dirs.map(shortPath).join(", ")}
                  </div>
                )}
                {session.integrity_warning && (
                  <div className="picker__session-warning" title={session.integrity_warning}>
                    <span className="picker__session-warning-label">⚠ Possibly spliced:</span> this
                    transcript may combine two unrelated sessions
                  </div>
                )}
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
    </>
  );
}

function noop() {}
