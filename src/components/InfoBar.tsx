import type { SessionMeta, SessionTotals, GitInfo } from "../types";
import {
  shortPath,
  shortMode,
  contextPercentFromTokens,
  formatTokens,
  formatCost,
} from "../lib/format";
import { getContextColor } from "../lib/theme";
import { TokensIcon, CostIcon } from "./Icons";

interface InfoBarProps {
  meta: SessionMeta;
  gitInfo: GitInfo | null;
  /** Latest Claude context-window fill (tokens); 0 hides the gauge. */
  contextTokens: number;
  sessionTotals: SessionTotals;
  sessionPath: string;
  ongoing: boolean;
}

export function InfoBar({
  meta,
  gitInfo,
  contextTokens,
  sessionTotals,
  sessionPath,
  ongoing,
}: InfoBarProps) {
  const projectName = shortPath(meta.cwd);
  const sessionId = sessionPath.split("/").pop()?.replace(".jsonl", "") || "";
  const branch = gitInfo?.branch || meta.git_branch;
  const dirty = gitInfo?.dirty ?? false;
  const mode = meta.permission_mode;
  const ctxPct = contextPercentFromTokens(contextTokens);

  const totalCost = sessionTotals.cost_usd;

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

      {mode && mode !== "default" && (
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

      {ongoing && (
        <span className="info-bar__ongoing">
          <span className="braille-spinner" /> active
        </span>
      )}
    </div>
  );
}
