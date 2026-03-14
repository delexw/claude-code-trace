import type { DisplayMessage } from "../types";
import { shortModel, formatExactTime, firstLine, truncate } from "../lib/format";
import { getModelColor } from "../lib/theme";
import { StatsBar, statsFromMessage } from "./StatsBar";
import { ClaudeIcon, UserIcon, SystemIcon, WarningIcon, ForwardIcon } from "./Icons";
import { OngoingDots } from "./OngoingDots";

interface MessageItemProps {
  message: DisplayMessage;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  onClick: (index: number) => void;
  onOpenDetail: (index: number) => void;
  /** Whether this message is currently being processed */
  isOngoing?: boolean;
  ref?: React.Ref<HTMLDivElement>;
}

export function MessageItem({
  ref,
  message: msg,
  index,
  isSelected,
  isExpanded,
  onClick,
  onOpenDetail,
  isOngoing,
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
      onDoubleClick={() => onOpenDetail(index)}
    >
      <div className="message__header">
        <span className="message__role-icon">
          {msg.role === "claude" ? (
            <ClaudeIcon className="message__claude-icon" />
          ) : msg.role === "user" ? (
            <UserIcon />
          ) : msg.is_error ? (
            <WarningIcon />
          ) : (
            <SystemIcon />
          )}
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
        {isOngoing && <OngoingDots />}
        {(msg.items.length > 0 || msg.tool_call_count > 0 || msg.thinking_count > 0) && (
          <button
            className="message__detail-btn"
            onClick={(e) => {
              e.stopPropagation();
              onOpenDetail(index);
            }}
          >
            Detail <ForwardIcon />
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
