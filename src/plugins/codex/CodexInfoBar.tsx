import { formatTokens } from "../../lib/format";
import type { CodexSession, CodexWindow } from "./types";
import { CodexCost } from "./CodexCost";

export function CodexInfoBar({ session, window }: { session: CodexSession; window: CodexWindow }) {
  return (
    <div className="info-bar">
      <span className="info-bar__project">{session.cwd || session.title || "Codex"}</span>
      <span className="info-bar__session-id">{session.id}</span>
      <span>{window.count} messages</span>
      {window.total_tokens > 0 && (
        <span className="info-bar__tokens">{formatTokens(window.total_tokens)} tokens</span>
      )}
      <CodexCost cost={window.cost_usd} className="info-bar__cost" />
      {session.archived && <span>Archived</span>}
    </div>
  );
}
