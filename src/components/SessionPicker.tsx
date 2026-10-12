import { useRef, useMemo } from "react";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { useVisibleSessions } from "../hooks/useVisibleSessions";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";
import type {
  EfficiencyAnalysisJob,
  EfficiencySummary,
  IndexProgress,
  SessionInfo,
  AntigravityBrainFolder,
  BrowseMode,
} from "../types";
import { OngoingDots } from "./OngoingDots";
import {
  formatTokens,
  formatDuration,
  formatExactTime,
  formatMsTimestamp,
  formatCost,
  truncate,
  groupByDate,
  shortModel,
  shortPath,
} from "../lib/format";
import { getModelColor } from "../lib/theme";
import { mergeRefs } from "../lib/mergeRefs";
import antigravityIcon from "../assets/antigravity.png";
import { selectableModes } from "../lib/browseMode";
import { BsClaude } from "react-icons/bs";
import { TokensIcon, CostIcon, ForwardIcon } from "./Icons";
import { BetaBadge } from "./BetaBadge";

const NO_EFFICIENCY_JOBS = new Map<string, EfficiencyAnalysisJob>();
const NO_EFFICIENCY_SUMMARIES = new Map<string, EfficiencySummary>();
const NO_ANTIGRAVITY_FOLDERS: AntigravityBrainFolder[] = [];

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

interface SessionPickerProps {
  sessions: SessionInfo[];
  /** How far the backend has got reading the project directories. */
  index: IndexProgress;
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
  /**
   * Shared toolbar-action registry. The picker registers Top/Bottom so they
   * scroll its own list; without it the toolbar's scroll buttons are inert here.
   */
  viewActionsRef?: ViewActionsRef;
  efficiencyJobs?: Map<string, EfficiencyAnalysisJob>;
  efficiencySummaries?: Map<string, EfficiencySummary>;
  onAnalyse?: (path: string) => void;
  onOpenEfficiencyDashboard?: (session: SessionInfo) => void;
  /**
   * Read-only Antigravity (Gemini IDE) brain folders, merged across every
   * installed version and already filtered against `searchQuery` by the
   * caller — App owns the filtering so the keyboard selection range and the
   * rendered list can never disagree. `browseMode` decides which of the two
   * lists is visible. Accepts null for callers that lazily hydrate the list.
   */
  antigravityFolders?: AntigravityBrainFolder[] | null;
  /** Triggered when the user clicks an Antigravity folder card. */
  onSelectAntigravity?: (folder: AntigravityBrainFolder) => void;
  /**
   * Which list to show. `antigravity` and `all` render the Antigravity
   * section; `claude` hides it and shows only sessions.
   */
  browseMode?: BrowseMode;
  /** Whether Antigravity is enabled at all — decides which modes are legal,
   *  since a stored "antigravity" must not survive the setting being turned
   *  off. The selector itself is drawn by `ViewToolbar`, which is also where a
   *  mode change is reported from. */
  antigravityEnabled?: boolean;
}

export function SessionPicker({
  sessions,
  index,
  loading,
  searchQuery,
  selectedIndex,
  onSelect,
  onSearchChange,
  onSelectIndex,
  onVisiblePathsChange,
  recapPreview = false,
  viewActionsRef,
  efficiencyJobs = NO_EFFICIENCY_JOBS,
  efficiencySummaries = NO_EFFICIENCY_SUMMARIES,
  onAnalyse,
  onOpenEfficiencyDashboard,
  antigravityFolders = NO_ANTIGRAVITY_FOLDERS,
  onSelectAntigravity,
  browseMode = "all",
  antigravityEnabled = false,
}: SessionPickerProps) {
  // Defensive: existing call sites (and tests) may pass `null` when they
  // haven't loaded the list yet. Treat it as empty rather than throwing.
  const antigravityList = antigravityFolders ?? NO_ANTIGRAVITY_FOLDERS;
  // A brain folder with no `transcript.jsonl` is not a conversation — it is a
  // scratch / artifacts directory that happens to live beside the ones that
  // are. The conversation *is* that file, so a folder without it has nothing to
  // open and no business in a conversation list. This hides 106 of 183 folders
  // on a real install; the "no transcript found" panel stays as the fallback
  // for a folder that loses its transcript between listing and opening.
  const conversationFolders = useMemo(
    () => antigravityList.filter((folder) => folder.has_transcript),
    [antigravityList],
  );
  // A mode persisted in localStorage can outlive the setting that made it
  // selectable (Antigravity turned off in Settings). Everything that decides
  // what to render must read the *clamped* mode, not the raw prop, or the
  // picker ends up showing neither list.
  const modes = selectableModes(antigravityEnabled);
  const effectiveMode = modes.some((m) => m.id === browseMode) ? browseMode : "claude";
  const showAntigravity = effectiveMode !== "claude" && antigravityEnabled;
  const showClaude = effectiveMode !== "antigravity";
  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedIndex);
  const searchRef = useRef<HTMLInputElement>(null);
  const registerVisible = useVisibleSessions(onVisiblePathsChange ?? noop);

  // Register Top/Bottom against the picker's own scroll container. Falls back to
  // a local ref when no registry is supplied (e.g. in isolated tests).
  const fallbackActionsRef = useRef({});
  useRegisterViewActions(viewActionsRef ?? fallbackActionsRef, {
    scrollToTop: () => listRef.current?.scrollTo({ top: 0, behavior: "smooth" }),
    scrollToBottom: () =>
      listRef.current?.scrollTo({ top: listRef.current.scrollHeight, behavior: "smooth" }),
  });

  const dateGroups = groupByDate(sessions);
  // Antigravity cards sit below the Claude list in the same scroll container,
  // so their indices continue past the last Claude card — one keyboard
  // selection spans both lists.
  const claudeCount = dateGroups.reduce((n, g) => n + g.items.length, 0);

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
        {/* The browse-mode selector (Claude / Antigravity / All) is not here —
            it lives in the view toolbar, which is where a view-level choice
            belongs. This list just obeys the mode it is given. */}
        <input
          ref={searchRef}
          className="picker__search"
          type="text"
          placeholder={
            effectiveMode === "antigravity" ? "Search conversations..." : "Search sessions..."
          }
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>

      <div className="picker__list" ref={listRef}>
        {loading && showClaude && (
          <div className="picker__loading">
            <span className="braille-spinner" />
            Discovering sessions...
          </div>
        )}

        {showClaude && !loading && sessions.length === 0 && index.done && (
          <div className="picker__empty">
            {searchQuery ? "No matching sessions" : "No sessions found"}
          </div>
        )}

        {/* Antigravity-only mode hides the Claude list; an empty state still
            needs to say something when that list is the one on screen. */}
        {!showClaude && !showAntigravity && (
          <div className="picker__empty">
            {antigravityEnabled
              ? "No conversations found"
              : "Enable Antigravity in Settings to browse its conversations"}
          </div>
        )}

        {showClaude &&
          dateGroups.map((group) => (
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
                  efficiencyJob &&
                  !["completed", "failed", "cancelled"].includes(efficiencyJob.status);
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
                        <span className="picker__session-warning-label">⚠ Possibly spliced:</span>{" "}
                        this transcript may combine two unrelated sessions
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

        {showAntigravity && conversationFolders.length === 0 && (
          <div className="picker__empty">
            {searchQuery ? "No matching conversations" : "No conversations found"}
          </div>
        )}

        {showAntigravity && conversationFolders.length > 0 && (
          <div className="picker__provider picker__provider--antigravity">
            <div className="picker__group-header picker__group-header--antigravity">
              <span className="picker__provider-badge picker__provider-badge--antigravity">
                Antigravity
              </span>
              <span className="picker__provider-count">{conversationFolders.length}</span>
              <span className="picker__provider-hint">read-only</span>
            </div>
            {conversationFolders.map((folder, agIndex) => {
              const idx = claudeCount + agIndex;
              const isSelected = idx === selectedIndex;
              return (
                <div
                  key={folder.path}
                  ref={isSelected ? selectedRef : undefined}
                  className={`picker__session picker__session--antigravity${isSelected ? " picker__session--selected" : ""}`}
                  role="button"
                  tabIndex={0}
                  aria-label={`Open Antigravity conversation ${folder.title ?? folder.uuid}`}
                  onMouseEnter={() => onSelectIndex?.(idx)}
                  onClick={() => onSelectAntigravity?.(folder)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelectAntigravity?.(folder);
                    }
                  }}
                >
                  <div className="picker__session-top">
                    <span className="picker__session-icon picker__session-icon--antigravity">
                      <img src={antigravityIcon} alt="" className="picker__session-icon-img" />
                    </span>
                    <span className="picker__session-preview">
                      {folder.title ? truncate(folder.title, 80) : "Untitled conversation"}
                    </span>
                    {/* The same affordance a Claude card carries. `Analyse` is
                        deliberately absent: efficiency scoring parses a Claude
                        transcript for usage/tool records a brain one does not
                        record, so the button could only ever error. */}
                    <button
                      className="message__detail-btn"
                      onClick={(event) => {
                        event.stopPropagation();
                        onSelectAntigravity?.(folder);
                      }}
                    >
                      Detail <ForwardIcon />
                    </button>
                  </div>
                  {/* Shaped like a Claude card: the icon and preview on top,
                      the facts underneath and hard right. A brain transcript
                      records no model, turn count, tokens or cost — only when
                      the conversation was last touched — and the flavour badge
                      a Claude card carries for its provider would be the only
                      mark on a card whose source is fixed. The root is still
                      one hover away, on the card's tooltip. */}
                  <div className="picker__session-meta">
                    {folder.mod_time_ms > 0 && (
                      <span className="picker__session-time">
                        {formatMsTimestamp(folder.mod_time_ms)}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function noop() {}
