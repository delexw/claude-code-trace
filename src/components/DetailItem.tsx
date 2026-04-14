import { useState } from "react";
import ReactMarkdown from "react-markdown";
import type { DisplayItem } from "../types";
import { formatDuration, formatJson, firstLine, truncate } from "../lib/format";
import { getTeamColor } from "../lib/theme";
import { StatsBar, useSubagentStats } from "./StatsBar";
import { PopoutModal } from "./PopoutModal";
import { OngoingDots } from "./OngoingDots";
import {
  toolCategoryIcons,
  ClaudeIcon,
  ThinkingIcon,
  OutputIcon,
  WarningIcon,
  HookIcon,
  DefaultItemIcon,
  ForwardIcon,
  PopoutIcon,
  ChevronIcon,
  PanelChevronIcon,
} from "./Icons";

interface DetailItemProps {
  item: DisplayItem;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  isAgentActive?: boolean;
  onToggle: (index: number, item: DisplayItem) => void;
  onToggleExpand: (index: number) => void;
  onSelect: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

export function DetailItem({
  ref,
  item,
  index,
  isSelected,
  isExpanded,
  isAgentActive,
  onToggle,
  onToggleExpand,
  onSelect,
}: DetailItemProps) {
  const icon = getItemIcon(item);
  const name = getItemName(item);
  const summary = getItemSummary(item);
  const teamClr = item.team_color ? getTeamColor(item.team_color) : undefined;
  const hasAgentMessages = item.subagent_messages.length > 0;
  const subagentStats = useSubagentStats(item);
  const [popout, setPopout] = useState(false);

  return (
    <div
      ref={ref}
      className={`detail-item${isSelected ? " detail-item--selected" : ""}${item.tool_error ? " detail-item--error" : ""}${isAgentActive ? " detail-item--agent-active" : ""}`}
      style={
        isAgentActive && teamClr
          ? { background: `${teamClr}12`, borderLeftColor: teamClr }
          : undefined
      }
      onDoubleClick={() => {
        if (hasAgentMessages) {
          onToggle(index, item);
        } else if (item.subagent_prompt) {
          onToggleExpand(index);
        }
      }}
    >
      <div
        className="detail-item__header"
        onClick={() => {
          onSelect(index);
          onToggle(index, item);
        }}
      >
        <span
          className={`detail-item__chevron${isExpanded ? " detail-item__chevron--expanded" : ""}${hasAgentMessages ? " detail-item__chevron--panel" : ""}`}
          style={hasAgentMessages && teamClr ? { color: teamClr } : undefined}
        >
          {hasAgentMessages ? <PanelChevronIcon /> : <ChevronIcon />}
        </span>
        <span className="detail-item__icon">{icon}</span>
        <span className="detail-item__name" style={teamClr ? { color: teamClr } : undefined}>
          {name}
        </span>
        <span className="detail-item__summary">{summary}</span>
        {item.is_orphan && <span className="detail-item__orphan-badge">orphan</span>}
        {item.agent_id && <span className="detail-item__agent-id">{item.agent_id}</span>}
        <span className="detail-item__right">
          {item.duration_ms > 0 && (
            <span className="detail-item__duration">{formatDuration(item.duration_ms)}</span>
          )}
          {item.subagent_ongoing && <OngoingDots />}
          {(hasAgentMessages || item.subagent_prompt) && (
            <button
              className="message__detail-btn"
              onClick={(e) => {
                e.stopPropagation();
                if (hasAgentMessages) {
                  onSelect(index);
                  onToggle(index, item);
                } else {
                  onToggleExpand(index);
                }
              }}
            >
              Detail <ForwardIcon />
            </button>
          )}
          {isExpanded && (
            <button
              className="detail-item__popout-btn"
              onClick={(e) => {
                e.stopPropagation();
                setPopout(true);
              }}
              title="Pop out to larger view"
            >
              <PopoutIcon />
            </button>
          )}
        </span>
      </div>
      {item.subagent_prompt && (
        <div
          className={`detail-item__prompt-preview${isExpanded ? " detail-item__prompt-preview--expanded" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand(index);
          }}
        >
          {firstLine(item.subagent_prompt)}
        </div>
      )}
      {subagentStats && <StatsBar stats={subagentStats} />}
      {isExpanded && <DetailItemBody item={item} />}
      {popout && (
        <PopoutModal
          onClose={() => setPopout(false)}
          header={
            <>
              <span className="popout-modal__icon">{icon}</span>
              <span className="popout-modal__name">{name}</span>
              {item.tool_summary && (
                <span className="popout-modal__summary">{item.tool_summary}</span>
              )}
              {item.agent_id && <span className="popout-modal__agent-id">{item.agent_id}</span>}
              <span className="popout-modal__right">
                {item.duration_ms > 0 && <span>{formatDuration(item.duration_ms)}</span>}
              </span>
            </>
          }
        >
          <DetailItemBody item={item} />
        </PopoutModal>
      )}
    </div>
  );
}

function DetailItemBody({ item }: { item: DisplayItem }) {
  switch (item.item_type) {
    case "Thinking":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text detail-item__text--thinking">
            {item.text || "Thinking content is not recorded in session logs."}
          </div>
        </div>
      );
    case "Output":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text detail-item__text--markdown">
            <ReactMarkdown>{item.text}</ReactMarkdown>
          </div>
        </div>
      );
    case "ToolCall":
      return (
        <div className="detail-item__body">
          {item.tool_input && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Input</div>
              <div className="detail-item__json">
                <pre>
                  <code>{formatJson(item.tool_input)}</code>
                </pre>
              </div>
            </div>
          )}
          {item.tool_result && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Result</div>
              <div
                className={`detail-item__text${item.tool_error ? " detail-item__text--error" : ""}`}
              >
                {item.tool_result}
              </div>
            </div>
          )}
        </div>
      );
    case "Subagent":
      return (
        <div className="detail-item__body">
          {item.agent_id && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Agent ID</div>
              <div className="detail-item__text detail-item__text--mono">{item.agent_id}</div>
            </div>
          )}
          {item.subagent_desc && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Description</div>
              <div className="detail-item__text">{item.subagent_desc}</div>
            </div>
          )}
          {item.subagent_prompt && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Prompt</div>
              <div className="detail-item__text">{item.subagent_prompt}</div>
            </div>
          )}
          {item.text && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Content</div>
              <div className="detail-item__text">{item.text}</div>
            </div>
          )}
        </div>
      );
    case "TeammateMessage":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text">{item.text}</div>
        </div>
      );
    case "HookEvent":
      return (
        <div className="detail-item__body">
          <div className="detail-item__section">
            <div className="detail-item__section-title">Hook</div>
            <div className="detail-item__text detail-item__text--mono">
              {item.hook_event}: {item.hook_name}
            </div>
          </div>
          {item.hook_command && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Command</div>
              <div className="detail-item__json">
                <pre>
                  <code>{item.hook_command}</code>
                </pre>
              </div>
            </div>
          )}
          {item.hook_metadata && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Metadata</div>
              <div className="detail-item__json">
                <pre>
                  <code>{item.hook_metadata}</code>
                </pre>
              </div>
            </div>
          )}
        </div>
      );
    default:
      return (
        <div className="detail-item__body">
          <div className="detail-item__text">{item.text}</div>
        </div>
      );
  }
}

export function getItemIcon(item: DisplayItem): React.ReactNode {
  switch (item.item_type) {
    case "Thinking":
      return <ThinkingIcon />;
    case "Output":
      return <OutputIcon />;
    case "ToolCall":
      return item.tool_error ? (
        <WarningIcon />
      ) : (
        (toolCategoryIcons[item.tool_category] ?? toolCategoryIcons.Other)
      );
    case "Subagent":
      return <ClaudeIcon className="detail-item__claude-icon" />;
    case "TeammateMessage":
      return <ClaudeIcon className="detail-item__claude-icon" />;
    case "HookEvent":
      return <HookIcon />;
    default:
      return <DefaultItemIcon />;
  }
}

export function getItemName(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking":
      return "Thinking";
    case "Output":
      return "Output";
    case "ToolCall":
      return item.tool_name || "Tool";
    case "Subagent":
      return item.subagent_type || "Subagent";
    case "TeammateMessage":
      return item.team_member_name || "Teammate";
    case "HookEvent":
      return item.hook_event || "Hook";
    default:
      return item.item_type;
  }
}

export function getItemSummary(item: DisplayItem): string {
  switch (item.item_type) {
    case "ToolCall":
      return item.tool_summary || "";
    case "Subagent":
      return item.subagent_desc || "";
    case "TeammateMessage":
      return item.text ? item.text.slice(0, 100) : "";
    case "Thinking":
      return item.text
        ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "")
        : "Content not recorded";
    case "Output":
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    case "HookEvent":
      return item.hook_name
        ? `${item.hook_name}${item.hook_command ? ": " + truncate(item.hook_command, 60) : ""}`
        : item.hook_command
          ? truncate(item.hook_command, 80)
          : "";
    default:
      return "";
  }
}
