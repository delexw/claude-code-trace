import { useCallback, useEffect, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";
import type { DisplayMessage } from "../types";
import { MessageItem } from "./MessageItem";

/** Decide how to scroll the selected message into view given the currently
 * rendered window. Returns `null` when no scroll is needed (selection is within
 * the window, or nothing is selected). Above the window → align to the top so
 * the message header stays visible; below → align to the end. */
export function selectionScrollTarget(
  selectedIndex: number,
  range: { startIndex: number; endIndex: number },
): { index: number; align: "start" | "end" } | null {
  if (selectedIndex < 0) return null;
  if (selectedIndex < range.startIndex) return { index: selectedIndex, align: "start" };
  if (selectedIndex > range.endIndex) return { index: selectedIndex, align: "end" };
  return null;
}

/** Per-render data threaded to the row renderer via Virtuoso's `context`, so the
 * renderer can stay a stable module-level function instead of an inline one. */
interface MessageRowContext {
  getMessage: (index: number) => DisplayMessage | undefined;
  roles: string[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
  lastIndex: number;
  onClick: (index: number) => void;
  onOpenDetail: (index: number) => void;
}

/** Placeholder for a row whose body hasn't been fetched yet. Uses the role from
 * the lightweight index so it reads correctly before the body loads. */
function roleModifier(role: string | undefined): string {
  if (role === "user") return "user";
  if (role === "claude") return "claude";
  if (role === "compact" || role === "recap") return "compact";
  return "system";
}

function renderMessageRow(index: number, _data: unknown, ctx: MessageRowContext) {
  const msg = ctx.getMessage(index);
  if (!msg) {
    // Height approximates a typical loaded row (header + a couple content
    // lines + stats bar) so the placeholder->content swap is a small reflow
    // instead of a large one — a big jump mid-scroll is what causes the
    // visible overlap/misalignment glitch on virtualized rows.
    return (
      <div className={`message message--placeholder message--${roleModifier(ctx.roles[index])}`}>
        <div className="message__placeholder-line message__placeholder-line--header" />
        <div className="message__placeholder-line" />
        <div className="message__placeholder-line message__placeholder-line--short" />
      </div>
    );
  }
  return (
    <MessageItem
      message={msg}
      index={index}
      isSelected={index === ctx.selectedIndex}
      isExpanded={ctx.expandedSet.has(index)}
      onClick={ctx.onClick}
      onOpenDetail={ctx.onOpenDetail}
      isOngoing={index === ctx.lastIndex && ctx.ongoing}
    />
  );
}

interface MessageListProps {
  /** Total number of messages in the session. */
  count: number;
  /** Fetch a loaded message body by absolute index (undefined if not loaded). */
  getMessage: (index: number) => DisplayMessage | undefined;
  /** Role of every message (length === count) — drives placeholders. */
  roles: string[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
  /** Ensure the bodies covering [start, end) are loaded. */
  onRangeChange: (start: number, end: number) => void;
  onSelect: (index: number) => void;
  onToggle: (index: number) => void;
  onOpenDetail: (index: number) => void;
  viewActionsRef: ViewActionsRef;
  onExpandAll: () => void;
  onCollapseAll: () => void;
}

export function MessageList({
  count,
  getMessage,
  roles,
  selectedIndex,
  expandedSet,
  ongoing,
  onRangeChange,
  onSelect,
  onToggle,
  onOpenDetail,
  viewActionsRef,
  onExpandAll,
  onCollapseAll,
}: MessageListProps) {
  const virtuosoRef = useRef<VirtuosoHandle>(null);

  // The message list is displayed reversed (newest first), so the visual top is
  // the last index and the visual bottom is index 0 — matching the keyboard
  // Top/Bottom jumps in App.
  useRegisterViewActions(viewActionsRef, {
    expandAll: onExpandAll,
    collapseAll: onCollapseAll,
    scrollToTop: () => virtuosoRef.current?.scrollToIndex(Math.max(0, count - 1)),
    scrollToBottom: () => virtuosoRef.current?.scrollToIndex(0),
  });
  // Track the currently rendered window so we only scroll when the selection
  // leaves it — mirrors the old "no-op if already visible" behaviour.
  const rangeRef = useRef({ startIndex: 0, endIndex: 0 });
  // Browsers only recompute `:hover` on a real pointer move, not when content
  // shifts under a stationary cursor during virtualized scrolling — so a stale
  // hover can appear "stuck" on whatever row happens to end up under the
  // cursor once scrolling stops. Suppressing hover/transitions while actively
  // scrolling (below) hides the glitch mid-scroll; forcing a synthetic
  // mousemove at the last known cursor position once scrolling settles (see
  // handleIsScrolling) fixes the stale state itself by making the browser
  // redo hit-testing without needing a real pointer move.
  const [isScrolling, setIsScrolling] = useState(false);
  const lastMouseRef = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const handleMove = (e: MouseEvent) => {
      lastMouseRef.current = { x: e.clientX, y: e.clientY };
    };
    window.addEventListener("mousemove", handleMove);
    return () => window.removeEventListener("mousemove", handleMove);
  }, []);

  const handleIsScrolling = useCallback((scrolling: boolean) => {
    setIsScrolling(scrolling);
    if (!scrolling && lastMouseRef.current) {
      const { x, y } = lastMouseRef.current;
      // Wait a frame so the settled scroll position has been painted before
      // the browser re-does hit-testing at these coordinates.
      requestAnimationFrame(() => {
        document.dispatchEvent(
          new MouseEvent("mousemove", { clientX: x, clientY: y, bubbles: true }),
        );
      });
    }
  }, []);

  const handleClick = useCallback(
    (index: number) => {
      if (selectedIndex === index) {
        onToggle(index);
      } else {
        onSelect(index);
      }
    },
    [selectedIndex, onSelect, onToggle],
  );

  // Bring the selected message into view on keyboard navigation. If it sits
  // above the window we align to the top (keep the header visible); below the
  // window we align to the end; already visible is a no-op.
  useEffect(() => {
    const target = selectionScrollTarget(selectedIndex, rangeRef.current);
    if (target) virtuosoRef.current?.scrollToIndex(target);
  }, [selectedIndex]);

  if (count === 0) {
    return (
      <div className="message-list">
        <div className="message-list__empty">No messages loaded</div>
      </div>
    );
  }

  return (
    <Virtuoso<unknown, MessageRowContext>
      ref={virtuosoRef}
      className={`message-list${isScrolling ? " message-list--scrolling" : ""}`}
      totalCount={count}
      initialTopMostItemIndex={Math.max(0, count - 1)}
      // Render rows this many pixels outside the viewport so their bodies load
      // and the placeholder->content swap happens off-screen, before the row
      // scrolls into view, instead of visibly reflowing at the viewport edge.
      increaseViewportBy={{ top: 600, bottom: 600 }}
      isScrolling={handleIsScrolling}
      context={{
        getMessage,
        roles,
        selectedIndex,
        expandedSet,
        ongoing,
        lastIndex: count - 1,
        onClick: handleClick,
        onOpenDetail,
      }}
      // Stick to the bottom on new/streamed content, but only while the user is
      // already at the bottom (replaces the old near-bottom auto-scroll hook).
      followOutput="smooth"
      rangeChanged={(range) => {
        rangeRef.current = range;
        onRangeChange(range.startIndex, range.endIndex + 1);
      }}
      itemContent={renderMessageRow}
    />
  );
}
