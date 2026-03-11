import { useRef, useEffect, useCallback } from "react";
import type { DisplayMessage } from "../types";
import {
  shortModel,
  formatTokens,
  formatDuration,
  formatTime,
  firstLine,
  truncate,
} from "../lib/format";
import { getModelColor, spinnerFrames } from "../lib/theme";

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
  const selectedRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const handleClick = useCallback(
    (index: number) => {
      if (selectedIndex === index) {
        onToggle(index);
      } else {
        onSelect(index);
      }
    },
    [selectedIndex, onSelect, onToggle]
  );

  const handleDoubleClick = useCallback(
    (index: number) => {
      onOpenDetail(index);
    },
    [onOpenDetail]
  );

  if (messages.length === 0) {
    return (
      <div className="message-list">
        <div className="message-list__empty">No messages loaded</div>
      </div>
    );
  }

  return (
    <div className="message-list" ref={listRef}>
      {messages.map((msg, i) => {
        if (msg.role === "compact") {
          return <CompactSeparator key={i} content={msg.content} />;
        }

        const isSelected = i === selectedIndex;
        const isExpanded = expandedSet.has(i);
        const isLast = i === messages.length - 1;

        return (
          <MessageItem
            key={i}
            ref={isSelected ? selectedRef : undefined}
            message={msg}
            index={i}
            isSelected={isSelected}
            isExpanded={isExpanded}
            isLastOngoing={isLast && ongoing}
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
  const time = formatTime(msg.timestamp);

  const contentPreview = isExpanded
    ? msg.content
    : truncate(firstLine(msg.content), 200);

  const hasStats =
    msg.tokens_raw > 0 ||
    msg.tool_call_count > 0 ||
    msg.thinking_count > 0 ||
    msg.duration_ms > 0;

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
          {msg.role === "user"
            ? "User"
            : msg.role === "claude"
              ? "Claude"
              : "System"}
        </span>
        {model && (
          <span className="message__model" style={{ color: modelColor }}>
            {model}
          </span>
        )}
        {msg.subagent_label && (
          <span className="detail-item__subagent-badge">
            {msg.subagent_label}
          </span>
        )}
        {time && <span className="message__timestamp">{time}</span>}
        {isLastOngoing && (
          <span className="message__ongoing-spinner">
            {spinnerFrames[animFrame % spinnerFrames.length]}
          </span>
        )}
      </div>

      <div
        className={`message__content${!isExpanded ? " message__content--collapsed" : ""}`}
      >
        {contentPreview}
      </div>

      {hasStats && (
        <div className="message__stats">
          {msg.tokens_raw > 0 && (
            <span
              className={`message__stat${msg.tokens_raw > 150000 ? " message__stat--tokens-high" : ""}`}
            >
              <span className="message__stat-icon">{"\u{1FA99}"}</span>
              {formatTokens(msg.tokens_raw)} tok
            </span>
          )}
          {msg.tool_call_count > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u{1F527}"}</span>
              {msg.tool_call_count} tool
              {msg.tool_call_count > 1 ? "s" : ""}
            </span>
          )}
          {msg.thinking_count > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u{1F4A1}"}</span>
              {msg.thinking_count} think
            </span>
          )}
          {msg.output_count > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u{1F4AC}"}</span>
              {msg.output_count} out
            </span>
          )}
          {msg.duration_ms > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u23F1"}</span>
              {formatDuration(msg.duration_ms)}
            </span>
          )}
          {msg.teammate_spawns > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u{1F916}"}</span>
              {msg.teammate_spawns} spawn
              {msg.teammate_spawns > 1 ? "s" : ""}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
