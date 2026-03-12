import { useRef, useCallback, useMemo } from "react";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import type { DisplayMessage } from "../types";
import { shortModel, formatExactTime, firstLine, truncate } from "../lib/format";
import { getModelColor, spinnerFrames } from "../lib/theme";
import { StatsBar, statsFromMessage } from "./StatsBar";

interface MessageListProps {
  messages: DisplayMessage[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
  animFrame: number;
  onSelect: (index: number) => void;
  onToggle: (index: number) => void;
  onOpenDetail: (index: number) => void;
}

export function MessageList({
  messages,
  selectedIndex,
  expandedSet,
  ongoing,
  animFrame,
  onSelect,
  onToggle,
  onOpenDetail,
}: MessageListProps) {
  const listRef = useRef<HTMLDivElement>(null);
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

  const handleDoubleClick = useCallback(
    (index: number) => {
      onOpenDetail(index);
    },
    [onOpenDetail],
  );

  // Chronological order: oldest messages first
  const ordered = useMemo(() => {
    const indices: number[] = [];
    for (let i = 0; i < messages.length; i++) {
      indices.push(i);
    }
    return indices;
  }, [messages.length]);

  if (messages.length === 0) {
    return (
      <div className="message-list">
        <div className="message-list__empty">No messages loaded</div>
      </div>
    );
  }

  return (
    <div className="message-list" ref={listRef}>
      {ordered.map((i) => {
        const msg = messages[i];
        if (msg.role === "compact") {
          return <CompactSeparator key={i} content={msg.content} />;
        }

        const isSelected = i === selectedIndex;
        const isExpanded = expandedSet.has(i);
        const isFirst = i === 0;

        return (
          <MessageItem
            key={i}
            ref={isSelected ? selectedRef : undefined}
            message={msg}
            index={i}
            isSelected={isSelected}
            isExpanded={isExpanded}
            isLastOngoing={isFirst && ongoing}
            animFrame={animFrame}
            onClick={handleClick}
            onDoubleClick={handleDoubleClick}
          />
        );
      })}
    </div>
  );
}

function CompactSeparator({ content }: { content: string }) {
  return (
    <div className="compact-separator">
      <div className="compact-separator__line">
        <span className="compact-separator__rule" />
        <span>{content}</span>
        <span className="compact-separator__rule" />
      </div>
    </div>
  );
}

interface MessageItemProps {
  message: DisplayMessage;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  isLastOngoing: boolean;
  animFrame: number;
  onClick: (index: number) => void;
  onDoubleClick: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

function MessageItem({
  ref,
  message: msg,
  index,
  isSelected,
  isExpanded,
  isLastOngoing,
  animFrame,
  onClick,
  onDoubleClick,
}: MessageItemProps) {
  const roleClass =
    msg.role === "user"
      ? "message--user"
      : msg.role === "claude"
        ? "message--claude"
        : msg.is_error
          ? "message--system-error"
          : "message--system";

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);

  const contentPreview = isExpanded ? msg.content : truncate(firstLine(msg.content), 200);

  const stats = statsFromMessage(msg);

  return (
    <div
      ref={ref}
      className={`message ${roleClass}${isSelected ? " message--selected" : ""}`}
      onClick={() => onClick(index)}
      onDoubleClick={() => onDoubleClick(index)}
    >
      <div className="message__header">
        <span className="message__role-icon">
          {msg.role === "user"
            ? "\u{1F464}"
            : msg.role === "claude"
              ? "\u{1F916}"
              : msg.is_error
                ? "\u26A0"
                : "\u{1F4BB}"}
        </span>
        <span
          className={`message__role message__role--${msg.role === "claude" ? "claude" : msg.role === "user" ? "user" : "system"}`}
        >
          {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
        </span>
        {model && (
          <span className="message__model" style={{ color: modelColor }}>
            {model}
          </span>
        )}
        {msg.subagent_label && (
          <span className="detail-item__subagent-badge">{msg.subagent_label}</span>
        )}
        {time && <span className="message__timestamp">{time}</span>}
        {isLastOngoing && (
          <span className="message__ongoing-spinner">
            {spinnerFrames[animFrame % spinnerFrames.length]}
          </span>
        )}
        {(msg.items.length > 0 || msg.tool_call_count > 0 || msg.thinking_count > 0) && (
          <button
            className="message__detail-btn"
            onClick={(e) => {
              e.stopPropagation();
              onDoubleClick(index);
            }}
          >
            Detail {"\u2192"}
          </button>
        )}
      </div>

      <div className={`message__content${!isExpanded ? " message__content--collapsed" : ""}`}>
        {contentPreview}
      </div>

      <StatsBar stats={stats} />
    </div>
  );
}
