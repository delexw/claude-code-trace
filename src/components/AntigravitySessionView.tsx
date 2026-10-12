import { useCallback, useMemo, useState } from "react";
import type { AntigravitySession } from "../types";
import { roleRender, toDisplayMessages } from "../lib/antigravityToDisplay";
import { useViewActionsRef, type ViewActionsRef } from "../hooks/useViewActions";
import { MessageDetail } from "./MessageDetail";
import { MessageList } from "./MessageList";

interface AntigravitySessionViewProps {
  session: AntigravitySession;
  /** The app-wide action registry the view toolbar drives. The viewer must
   *  share it: a ref of its own leaves Expand All / Collapse All / Top /
   *  Bottom calling whichever list was mounted before. Falls back to a local
   *  one so the component still stands alone. */
  viewActionsRef?: ViewActionsRef;
  /** Root CSS zoom, mirrored from the Claude list view: Virtuoso caches row
   *  geometry, so it has to remount at the new scale to re-measure in one
   *  coordinate space. */
  fontScale?: number;
}

/**
 * Read-only viewer for an Antigravity conversation.
 *
 * The rows are drawn by the Claude `MessageList` / `MessageItem`, so a brain
 * transcript gets the same virtualization, stats bar, `Detail` button and
 * expand-all as a Claude session. Two things stay deliberately different:
 * `rowRoleRender` keeps the per-step border colours and the nine Antigravity
 * role labels, and the stats come from what the transcript actually records —
 * tool calls, thinking blocks and step durations — with token counts left at
 * zero rather than invented, which the stats bar simply hides.
 *
 * The conversation arrives whole, in one read. It was once paged by offset;
 * that matched a whole-file read exactly across every conversation on disk,
 * but did not hold up in the running app, so the simpler path is the one.
 */
export function AntigravitySessionView({
  session,
  viewActionsRef,
  fontScale,
}: AntigravitySessionViewProps) {
  const { folder, messages, total_steps } = session;
  const [selectedIndex, setSelectedIndex] = useState<number>(-1);
  const [expandedSet, setExpandedSet] = useState<Set<number>>(() => new Set([0]));
  const [detailIndex, setDetailIndex] = useState<number | null>(null);
  const fallbackActionsRef = useViewActionsRef();
  const actionsRef = viewActionsRef ?? fallbackActionsRef;

  const display = useMemo(() => toDisplayMessages(messages), [messages]);
  const roles = useMemo(() => messages.map((m) => m.role), [messages]);
  const getMessage = useCallback((index: number) => display[index], [display]);
  const rowRoleRender = useCallback(
    (index: number) => {
      const message = messages[index];
      return message ? roleRender(message.role) : undefined;
    },
    [messages],
  );

  const onSelect = useCallback((index: number) => setSelectedIndex(index), []);

  const onToggle = useCallback((index: number) => {
    setExpandedSet((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  const onExpandAll = useCallback(
    () => setExpandedSet(new Set(messages.map((_, i) => i))),
    [messages],
  );
  const onCollapseAll = useCallback(() => setExpandedSet(new Set()), []);
  // Everything arrived at once, so there is no window to request.
  const onRangeChange = useCallback(() => {}, []);

  if (detailIndex !== null && display[detailIndex]) {
    const step = messages[detailIndex];
    return (
      <MessageDetail
        message={display[detailIndex]}
        onBack={() => setDetailIndex(null)}
        viewActionsRef={actionsRef}
        roleLabel={step ? roleRender(step.role).label : undefined}
      />
    );
  }

  if (messages.length === 0) {
    return (
      <div className="message-list message-list--antigravity-empty">
        <div className="message-list__empty">
          {total_steps === 0 ? (
            <>
              No transcript found at{" "}
              <code>{folder.path}/.system_generated/logs/transcript.jsonl</code>
            </>
          ) : (
            // The transcript exists — every step in it recorded nothing.
            <>None of the {total_steps} steps in this conversation recorded any text.</>
          )}
        </div>
      </div>
    );
  }

  // No topbar of its own: the view toolbar already carries the way back, the
  // conversation title lives in the info bar above, and the step count sits
  // with Expand/Collapse. A second header repeated all three.
  return (
    <div className="message-list message-list--antigravity">
      <MessageList
        key={fontScale}
        count={messages.length}
        getMessage={getMessage}
        roles={roles}
        selectedIndex={selectedIndex}
        expandedSet={expandedSet}
        ongoing={false}
        onRangeChange={onRangeChange}
        onSelect={onSelect}
        onToggle={onToggle}
        onOpenDetail={setDetailIndex}
        viewActionsRef={actionsRef}
        onExpandAll={onExpandAll}
        onCollapseAll={onCollapseAll}
        rowRoleRender={rowRoleRender}
      />
    </div>
  );
}
