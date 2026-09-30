import type { AntigravityMessage } from "../types";
import { formatExactTime, firstNLines } from "../lib/format";
import { ClaudeIcon, UserIcon, SystemIcon, WarningIcon } from "./Icons";

interface AntigravityMessageItemProps {
  message: AntigravityMessage;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  onClick: (index: number) => void;
}

function roleClass(role: AntigravityMessage["role"]): string {
  switch (role) {
    case "user":
      return "message--user";
    case "assistant":
      return "message--claude";
    case "tool":
      return "message--tool";
    case "file":
      return "message--file";
    case "search":
      return "message--search";
    case "error":
      return "message--system-error";
    case "checkpoint":
      return "message--compact";
    case "context":
      return "message--context";
    default:
      return "message--system";
  }
}

function roleLabel(role: AntigravityMessage["role"]): string {
  switch (role) {
    case "user":
      return "User";
    case "assistant":
      return "Assistant";
    case "tool":
      return "MCP Tool";
    case "file":
      return "View File";
    case "search":
      return "Grep Search";
    case "error":
      return "Error";
    case "checkpoint":
      return "Checkpoint";
    case "context":
      return "Context";
    default:
      return "System";
  }
}

function roleCssModifier(role: AntigravityMessage["role"]): string {
  switch (role) {
    case "user":
      return "user";
    case "assistant":
      return "claude";
    case "tool":
      return "tool";
    case "file":
      return "file";
    case "search":
      return "search";
    case "error":
      return "system-error";
    case "checkpoint":
      return "compact";
    case "context":
      return "context";
    default:
      return "system";
  }
}

/**
 * Renders a single normalized Antigravity transcript step. Intentionally
 * mirrors `MessageItem.tsx` (same CSS classes, same header/content/stats
 * shape) so the Antigravity viewer reads visually like the Claude one
 * while keeping the wire-format content intact — no embellishment.
 */
export function AntigravityMessageItem({
  message,
  index,
  isSelected,
  isExpanded,
  onClick,
}: AntigravityMessageItemProps) {
  const rc = roleClass(message.role);
  const time = message.created_at ? formatExactTime(message.created_at) : "";
  const contentPreview = isExpanded ? message.content : firstNLines(message.content, 12);

  return (
    <div
      className={`message ${rc}${isSelected ? " message--selected" : ""}`}
      onClick={() => onClick(index)}
    >
      <div className="message__header">
        <span className="message__role-icon">
          {message.role === "assistant" ? (
            <ClaudeIcon className="message__claude-icon" />
          ) : message.role === "user" ? (
            <UserIcon />
          ) : message.role === "error" ? (
            <WarningIcon />
          ) : (
            <SystemIcon />
          )}
        </span>
        <span className={`message__role message__role--${roleCssModifier(message.role)}`}>
          {roleLabel(message.role)}
        </span>
        {message.tool_name && <span className="message__model">{message.tool_name}</span>}
        <span className="message__step">#{message.step_index}</span>
        {time && <span className="message__timestamp">{time}</span>}
      </div>

      <div className={`message__content${!isExpanded ? " message__content--collapsed" : ""}`}>
        {contentPreview}
      </div>
    </div>
  );
}
