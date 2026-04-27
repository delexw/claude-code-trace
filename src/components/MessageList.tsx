import { useCallback, useMemo } from "react";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { useAutoScroll } from "../hooks/useAutoScroll";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";
import type { DisplayMessage } from "../types";
import { MessageItem } from "./MessageItem";

interface MessageListProps {
  messages: DisplayMessage[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
  onSelect: (index: number) => void;
  onToggle: (index: number) => void;
  onOpenDetail: (index: number) => void;
  viewActionsRef: ViewActionsRef;
  onExpandAll: () => void;
  onCollapseAll: () => void;
}

export function MessageList({
  messages,
  selectedIndex,
  expandedSet,
  ongoing,
  onSelect,
  onToggle,
  onOpenDetail,
  viewActionsRef,
  onExpandAll,
  onCollapseAll,
}: MessageListProps) {
  useRegisterViewActions(viewActionsRef, { expandAll: onExpandAll, collapseAll: onCollapseAll });
  const listRef = useAutoScroll<HTMLDivElement>(messages.length);
  const selectedRef = useScrollToSelected(selectedIndex);

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

  // Chronological order: oldest messages first
  const ordered = useMemo(() => {
    const indices: number[] = [];
    for (let i = 0; i < messages.length; i++) {
      indices.push(i);
    }
    return indices;
  }, [messages.length]);

  return (
    <div className="message-list" ref={listRef}>
      {messages.length === 0 && <div className="message-list__empty">No messages loaded</div>}
      {ordered.map((i) => {
        const msg = messages[i];
        const isSelected = i === selectedIndex;
        const isLast = i === messages.length - 1;

        return (
          <MessageItem
            key={i}
            ref={isSelected ? selectedRef : undefined}
            message={msg}
            index={i}
            isSelected={isSelected}
            isExpanded={expandedSet.has(i)}
            onClick={handleClick}
            onOpenDetail={onOpenDetail}
            isOngoing={isLast && ongoing}
          />
        );
      })}
    </div>
  );
}
