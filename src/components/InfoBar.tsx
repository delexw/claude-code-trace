import { useMemo } from "react";
import type { DisplayMessage, SessionMeta, GitInfo } from "../types";
import { shortPath, shortMode, contextPercent } from "../lib/format";
import { getContextColor, spinnerFrames } from "../lib/theme";

interface InfoBarProps {
  meta: SessionMeta;
  gitInfo: GitInfo | null;
  messages: DisplayMessage[];
  ongoing: boolean;
  animFrame?: number;
}

export function InfoBar({
  meta,
  gitInfo,
  messages,
  ongoing,
  animFrame = 0,
}: InfoBarProps) {
  const projectName = shortPath(meta.cwd, meta.git_branch);
  const branch = gitInfo?.branch || meta.git_branch;
  const dirty = gitInfo?.dirty ?? false;
  const mode = meta.permission_mode;
  const ctxPct = useMemo(() => contextPercent(messages), [messages]);

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
      {projectName && (
        <span className="info-bar__project">{projectName}</span>
      )}

      {branch && (
        <span
          className={`info-bar__branch${dirty ? " info-bar__branch--dirty" : ""}`}
        >
          {branch}
        </span>
      )}

      {mode && mode !== "default" && (
        <span className={`info-bar__pill ${pillClass}`}>
          {shortMode(mode)}
        </span>
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

      {ongoing && (
        <span className="info-bar__ongoing">
          {spinnerFrames[animFrame % spinnerFrames.length]} active
        </span>
      )}
    </div>
  );
}
