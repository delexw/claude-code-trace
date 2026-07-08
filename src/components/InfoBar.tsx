import type { SessionMeta, SessionTotals, GitInfo, SessionInfo } from "../types";
import {
  shortPath,
  shortMode,
  isDefaultMode,
  contextPercentFromTokens,
  formatTokens,
  formatCost,
} from "../lib/format";
import { getContextColor } from "../lib/theme";
import { TokensIcon, CostIcon } from "./Icons";
import { SessionActions } from "./SessionActions";

/** Format a session's liveness into the status-row badge text, or `null` when
 *  there's no liveness info. Moved here (verbatim) from `SessionActions` —
 *  liveness is the authoritative real-time status signal, so it now lives
 *  alongside the (mutually exclusive) `active`/`info-bar__ongoing` indicator. */
function badge(l: SessionInfo["liveness"]): string | null {
  if (!l) return null;
  if (l.status === "busy") return "● busy";
  if (l.status === "idle") return `○ idle ${Math.floor(l.idle_seconds / 60)}m`;
  return `○ ${l.status}`; // forward-compat: render an unknown status, don't assume the set is closed
}

interface InfoBarProps {
  meta: SessionMeta;
  gitInfo: GitInfo | null;
  /** Latest Claude context-window fill (tokens); 0 hides the gauge. */
  contextTokens: number;
  sessionTotals: SessionTotals;
  sessionPath: string;
  ongoing: boolean;
  /** Full SessionInfo for the open session (liveness, session_id), when known
   *  from the picker's session list; null while it hasn't loaded/matched yet. */
  sessionInfo?: SessionInfo | null;
  /** Whether this backend can focus a session's terminal window. */
  canFocus: boolean;
}

export function InfoBar({
  meta,
  gitInfo,
  contextTokens,
  sessionTotals,
  sessionPath,
  ongoing,
  sessionInfo,
  canFocus,
}: InfoBarProps) {
  const projectName = shortPath(meta.cwd);
  const sessionId = sessionPath.split("/").pop()?.replace(".jsonl", "") || "";
  const branch = gitInfo?.branch || meta.git_branch;
  const dirty = gitInfo?.dirty ?? false;
  const mode = meta.permission_mode;
  const ctxPct = contextPercentFromTokens(contextTokens);

  const totalCost = sessionTotals.cost_usd;
  // Liveness (from the session registry) is the more authoritative real-time
  // signal — when present it supersedes the transcript-derived `ongoing`
  // indicator so only one status signal is ever shown at once.
  const liveness = sessionInfo?.liveness;
  const livenessBadge = badge(liveness);

  const pillClass =
    mode === "bypassPermissions"
      ? "info-bar__pill--bypass"
      : mode === "acceptEdits"
        ? "info-bar__pill--acceptEdits"
        : mode === "plan"
          ? "info-bar__pill--plan"
          : "info-bar__pill--default";

  return (
    <div className="info-bar">
      {projectName && <span className="info-bar__project">{projectName}</span>}

      {sessionId && <span className="info-bar__session-id">{sessionId}</span>}

      {branch && (
        <span className={`info-bar__branch${dirty ? " info-bar__branch--dirty" : ""}`}>
          {branch}
        </span>
      )}

      {mode && !isDefaultMode(mode) && (
        <span className={`info-bar__pill ${pillClass}`}>{shortMode(mode)}</span>
      )}

      {ctxPct >= 0 && (
        <div className="info-bar__context">
          <span>ctx {ctxPct}%</span>
          <div className="info-bar__context-bar">
            <div
              className="info-bar__context-fill"
              style={{
                width: `${ctxPct}%`,
                backgroundColor: getContextColor(ctxPct),
              }}
            />
          </div>
        </div>
      )}

      {sessionTotals.total_tokens > 0 && (
        <span className="info-bar__tokens">
          <TokensIcon /> {formatTokens(sessionTotals.total_tokens)} tok
        </span>
      )}
      {totalCost > 0 && (
        <span className="info-bar__cost">
          <CostIcon /> {formatCost(totalCost)}
        </span>
      )}

      {livenessBadge ? (
        <span className="info-bar__status-badge">{livenessBadge}</span>
      ) : (
        ongoing && (
          <span className="info-bar__ongoing">
            <span className="braille-spinner" /> active
          </span>
        )
      )}

      {sessionInfo && <SessionActions session={sessionInfo} canFocus={canFocus} />}
    </div>
  );
}
