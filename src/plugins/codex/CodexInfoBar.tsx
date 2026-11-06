import { formatDuration, formatTokens } from "../../lib/format";
import { getContextColor } from "../../lib/theme";
import type { CodexSession, CodexWindow } from "./types";
import { CodexCost } from "./CodexCost";
import { CodexSessionActions } from "./CodexSessionActions";
import type { GitInfo } from "../../types";

export function CodexInfoBar({
  session,
  window,
  gitInfo,
}: {
  session: CodexSession;
  window: CodexWindow;
  gitInfo?: GitInfo | null;
}) {
  const model = window.model || session.model;
  const branch = gitInfo?.branch || window.git_branch || session.git_branch;
  const permission = window.permission_mode || session.permission_mode;
  const sandbox = window.sandbox_policy || session.sandbox_policy;
  const context = window.context_tokens;
  const capacity = window.context_window;
  const percent =
    context != null && capacity != null && capacity > 0
      ? Math.min(100, Math.max(0, Math.round((context / capacity) * 100)))
      : null;
  return (
    <div className="info-bar">
      <span className="info-bar__project">{session.cwd || session.title || "Codex"}</span>
      <span className="info-bar__session-id">{session.id}</span>
      {branch && (
        <span
          className={`info-bar__branch${gitInfo?.dirty ? " info-bar__branch--dirty" : ""}`}
          title={gitInfo?.branch ? "Current Git branch" : "Recorded Git branch"}
        >
          {branch}
        </span>
      )}
      {model && <span>{model}</span>}
      {permission && (
        <span className="info-bar__pill" title="Codex approval policy">
          {permission}
        </span>
      )}
      {sandbox && (
        <span className="info-bar__pill" title="Codex sandbox policy">
          {sandbox}
        </span>
      )}
      {context != null && (
        <div
          className="info-bar__context"
          title={`Latest context: ${formatTokens(context)} tokens${capacity ? ` / ${formatTokens(capacity)}` : ""}`}
        >
          <span>{percent == null ? `ctx ${formatTokens(context)}` : `ctx ${percent}%`}</span>
          {percent != null && (
            <div className="info-bar__context-bar">
              <div
                className="info-bar__context-fill"
                style={{ width: `${percent}%`, backgroundColor: getContextColor(percent) }}
              />
            </div>
          )}
        </div>
      )}
      <span>{window.count} messages</span>
      {window.duration_ms != null && (
        <span title="Recorded active turn time">{formatDuration(window.duration_ms)}</span>
      )}
      {window.total_tokens > 0 && (
        <span className="info-bar__tokens">{formatTokens(window.total_tokens)} tokens</span>
      )}
      <CodexCost cost={window.cost_usd} partial={window.cost_partial} className="info-bar__cost" />
      {session.archived && <span>Archived</span>}
      {window.ongoing && (
        <span className="info-bar__ongoing">
          <span className="braille-spinner" /> active
        </span>
      )}
      <CodexSessionActions key={session.path} session={session} />
    </div>
  );
}
