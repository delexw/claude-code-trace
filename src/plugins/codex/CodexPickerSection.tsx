import { Fragment } from "react";
import { useScrollToSelected } from "../../hooks/useScrollToSelected";
import { ForwardIcon, TokensIcon } from "../../components/Icons";
import {
  formatDuration,
  formatMsTimestamp,
  formatTokens,
  groupByDate,
  truncate,
} from "../../lib/format";
import { OngoingDots } from "../../components/OngoingDots";
import { CodexCost } from "./CodexCost";
import { CodexIcon } from "./roleStyles";
import type { CodexSession } from "./types";
import { filterCodexProjectSessions } from "./projects";

export function visibleSessions(
  sessions: CodexSession[],
  searchQuery: string,
  selectedProject?: string | null,
): CodexSession[] {
  const query = searchQuery.trim().toLowerCase();
  const filtered = filterCodexProjectSessions(sessions, selectedProject).filter(
    (session) =>
      !query ||
      [
        session.title,
        session.id,
        session.cwd,
        session.model,
        session.source,
        session.archived ? "archived" : "",
      ].some((value) => value.toLowerCase().includes(query)),
  );
  return groupByDate(
    filtered.map((session) => ({ session, mod_time: session.modified_at })),
  ).flatMap((group) => group.items.map(({ session }) => session));
}

interface Props {
  sessions: CodexSession[];
  loading: boolean;
  error: string | null;
  searchQuery: string;
  selectedIndex: number | null;
  onHover(index: number): void;
  onOpen(session: CodexSession): void;
  onRefresh(): void;
}

export function CodexPickerSection({
  sessions,
  loading,
  error,
  searchQuery,
  selectedIndex,
  onHover,
  onOpen,
  onRefresh,
}: Props) {
  const selectedRef = useScrollToSelected(selectedIndex ?? -1);
  const dateGroups = groupByDate(
    sessions.map((session) => ({ session, mod_time: session.modified_at })),
  );
  let flatIndex = 0;
  return (
    <div className="picker__section codex-section">
      <div className="picker__section-header">
        <span className="codex-section__badge">Codex</span>
        <span className="picker__section-count">{sessions.length}</span>
        <button className="view-toolbar__btn" onClick={onRefresh} disabled={loading}>
          Refresh
        </button>
      </div>
      {error && (
        <div className="codex-error" role="alert">
          {error}
        </div>
      )}
      {loading && sessions.length === 0 && (
        <div className="picker__loading">
          <span className="braille-spinner" />
          Reading Codex chats...
        </div>
      )}
      {!loading && sessions.length === 0 && (
        <div className="picker__empty">
          {searchQuery.trim()
            ? "No matching Codex chats"
            : "No Codex chats found. Check the Codex home directory in Settings."}
        </div>
      )}
      {dateGroups.map((group) => (
        <Fragment key={group.category}>
          <div className="picker__group-header">{group.category}</div>
          {group.items.map(({ session }) => {
            const index = flatIndex++;
            return (
              <div
                key={session.path}
                ref={index === selectedIndex ? selectedRef : undefined}
                className={`picker__session codex-section__card${index === selectedIndex ? " picker__session--selected" : ""}${session.is_ongoing ? " picker__session--ongoing" : ""}`}
                title={session.path}
                onMouseEnter={() => onHover(index)}
                onClick={() => onOpen(session)}
              >
                <div className="picker__session-top">
                  <span className="picker__session-icon">
                    <CodexIcon />
                  </span>
                  <span className="picker__session-preview">
                    {truncate(session.title || "Untitled Codex chat", 100)}
                  </span>
                  {session.is_ongoing && (
                    <span title="Codex is working">
                      <OngoingDots count={1} />
                    </span>
                  )}
                  <button
                    className="message__detail-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpen(session);
                    }}
                  >
                    Detail <ForwardIcon />
                  </button>
                </div>
                <div className="picker__session-meta">
                  {session.cwd && <span>{session.cwd}</span>}
                  {session.model && <span>{session.model}</span>}
                  {session.git_branch && <span title="Git branch">{session.git_branch}</span>}
                  {session.permission_mode && (
                    <span title="Approval policy">{session.permission_mode}</span>
                  )}
                  {session.sandbox_policy && (
                    <span title="Sandbox policy">{session.sandbox_policy}</span>
                  )}
                  <span>{session.source}</span>
                  {(session.turn_count ?? 0) > 0 && <span>{session.turn_count} turns</span>}
                  {session.duration_ms != null && (
                    <span>{formatDuration(session.duration_ms)}</span>
                  )}
                  {session.total_tokens > 0 && (
                    <span className="picker__session-stat">
                      <TokensIcon /> {formatTokens(session.total_tokens)} tok
                    </span>
                  )}
                  <CodexCost
                    cost={session.cost_usd}
                    partial={session.cost_partial}
                    className="picker__session-stat picker__session-stat--cost"
                  />
                  {session.archived && <span>Archived</span>}
                  {Number.isFinite(Date.parse(session.modified_at)) && (
                    <span className="picker__session-time">
                      {formatMsTimestamp(Date.parse(session.modified_at))}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
