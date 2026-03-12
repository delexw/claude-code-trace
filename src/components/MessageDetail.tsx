import { useState, useMemo, useCallback, useRef } from "react";
import ReactMarkdown from "react-markdown";
import type { DisplayMessage, DisplayItem } from "../types";
import {
  shortModel,
  formatTokens,
  formatDuration,
  formatExactTime,
  formatJson,
  firstLine,
  truncate,
} from "../lib/format";
import { getModelColor, getTeamColor, toolCategoryIcons } from "../lib/theme";
import { useToggleSet } from "../hooks/useToggleSet";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { BackButton } from "./BackButton";
import { PopoutModal } from "./PopoutModal";

interface MessageDetailProps {
  message: DisplayMessage;
  onBack: () => void;
}

export function MessageDetail({ message: msg, onBack }: MessageDetailProps) {
  const { set: expandedItems, toggle: toggleItem } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const [agentPanel, setAgentPanel] = useState<DisplayItem | null>(null);

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);

  const hasItems = msg.items.length > 0;

  const handleItemClick = (index: number, item: DisplayItem) => {
    setSelectedItem(index);
    if (item.item_type === "Subagent" && item.subagent_messages.length > 0) {
      setAgentPanel(agentPanel?.agent_id === item.agent_id ? null : item);
    } else {
      toggleItem(index);
    }
  };

  return (
    <div className={`message-detail${agentPanel ? " message-detail--split" : ""}`}>
      <div className="message-detail__main">
        <div className="message-detail__header">
          <BackButton onClick={onBack} />
          <span className="message-detail__title">
            {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
          </span>
          {model && <span style={{ color: modelColor, fontWeight: 600, fontSize: 12 }}>{model}</span>}
          {msg.subagent_label && (
            <span className="detail-item__subagent-badge">{msg.subagent_label}</span>
          )}
          <span className="message-detail__meta">
            {time}
            {msg.tokens_raw > 0 && (
              <>
                {" "}
                {"\u00B7"} {formatTokens(msg.tokens_raw)} tok
              </>
            )}
            {msg.duration_ms > 0 && (
              <>
                {" "}
                {"\u00B7"} {formatDuration(msg.duration_ms)}
              </>
            )}
          </span>
        </div>

        <div className="message-detail__body">
          <div className="message-detail__content">
            {msg.content && (
              <div className="message-detail__text">
                <ReactMarkdown>{msg.content}</ReactMarkdown>
              </div>
            )}

            {hasItems && (
              <div className="detail-items">
                <div className="detail-items__section-label">Items ({msg.items.length})</div>
                {msg.items.map((item, idx) => (
                  <DetailItem
                    key={idx}
                    ref={idx === selectedItem ? scrollRef : undefined}
                    item={item}
                    index={idx}
                    isSelected={idx === selectedItem}
                    isExpanded={item.item_type !== "Subagent" && expandedItems.has(idx)}
                    isAgentActive={agentPanel?.agent_id === item.agent_id && item.item_type === "Subagent" && !!item.agent_id}
                    onToggle={handleItemClick}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {agentPanel && (
        <AgentPanel item={agentPanel} onClose={() => setAgentPanel(null)} />
      )}
    </div>
  );
}

/* ─── Agent panel: same layout as main message list + detail ─── */

interface AgentPanelProps {
  item: DisplayItem;
  onClose: () => void;
}

function AgentPanel({ item, onClose }: AgentPanelProps) {
  const messages = item.subagent_messages;
  const [selectedMsg, setSelectedMsg] = useState(messages.length - 1);
  const { set: expandedSet, toggle: toggleMsg, clear: clearExpanded } = useToggleSet();
  const [detailMsg, setDetailMsg] = useState<DisplayMessage | null>(null);
  const prevAgentId = useRef(item.agent_id);

  // Reset state when switching to a different agent
  if (item.agent_id !== prevAgentId.current) {
    prevAgentId.current = item.agent_id;
    setSelectedMsg(messages.length - 1);
    setDetailMsg(null);
    clearExpanded();
  }

  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedMsg);

  const handleClick = useCallback(
    (index: number) => {
      if (selectedMsg === index) {
        toggleMsg(index);
      } else {
        setSelectedMsg(index);
      }
    },
    [selectedMsg, toggleMsg],
  );

  const reversed = useMemo(() => {
    const indices: number[] = [];
    for (let i = messages.length - 1; i >= 0; i--) indices.push(i);
    return indices;
  }, [messages.length]);

  return (
    <div className="agent-panel">
      <div className="agent-panel__header">
        <button className="agent-panel__close" onClick={onClose}>{"\u2715"}</button>
        <span className="agent-panel__icon">{"\u{1F916}"}</span>
        <span className="agent-panel__type">{item.subagent_type || "Subagent"}</span>
        {item.subagent_desc && <span className="agent-panel__desc">{item.subagent_desc}</span>}
        {item.agent_id && <span className="agent-panel__id">{item.agent_id}</span>}
        <span className="agent-panel__stats">
          {item.duration_ms > 0 && <span>{formatDuration(item.duration_ms)}</span>}
          {item.token_count > 0 && <span>{formatTokens(item.token_count)} tok</span>}
        </span>
      </div>

      <div className="agent-panel__content">
        {detailMsg ? (
          <AgentDetailView msg={detailMsg} onBack={() => setDetailMsg(null)} />
        ) : (
          <div className="agent-panel__list" ref={listRef}>
            {reversed.map((i) => {
              const msg = messages[i];
              if (msg.role === "compact") {
                return (
                  <div key={i} className="compact-separator">
                    <div className="compact-separator__line">
                      <span className="compact-separator__rule" />
                      <span>{msg.content}</span>
                      <span className="compact-separator__rule" />
                    </div>
                  </div>
                );
              }

              const isSelected = i === selectedMsg;
              const isExpanded = expandedSet.has(i);

              return (
                <AgentMessageItem
                  key={i}
                  ref={isSelected ? selectedRef : undefined}
                  msg={msg}
                  index={i}
                  isSelected={isSelected}
                  isExpanded={isExpanded}
                  onClick={handleClick}
                  onOpenDetail={(idx) => {
                    setDetailMsg(messages[idx]);
                  }}
                />
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

/* Agent message item — same structure as main MessageItem */
interface AgentMessageItemProps {
  msg: DisplayMessage;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  onClick: (index: number) => void;
  onOpenDetail: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

function AgentMessageItem({ ref, msg, index, isSelected, isExpanded, onClick, onOpenDetail }: AgentMessageItemProps) {
  const roleClass =
    msg.role === "user" ? "message--user"
      : msg.role === "claude" ? "message--claude"
        : msg.is_error ? "message--system-error"
          : "message--system";

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);
  const contentPreview = isExpanded ? msg.content : truncate(firstLine(msg.content), 200);

  const subagentCount = msg.items.filter((it) => it.item_type === "Subagent").length;
  const hasStats = msg.tokens_raw > 0 || msg.tool_call_count > 0 || msg.thinking_count > 0 || msg.duration_ms > 0 || subagentCount > 0;

  return (
    <div
      ref={ref}
      className={`message ${roleClass}${isSelected ? " message--selected" : ""}`}
      onClick={() => onClick(index)}
      onDoubleClick={() => onOpenDetail(index)}
    >
      <div className="message__header">
        <span className="message__role-icon">
          {msg.role === "user" ? "\u{1F464}" : msg.role === "claude" ? "\u{1F916}" : msg.is_error ? "\u26A0" : "\u{1F4BB}"}
        </span>
        <span className={`message__role message__role--${msg.role === "claude" ? "claude" : msg.role === "user" ? "user" : "system"}`}>
          {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
        </span>
        {model && <span className="message__model" style={{ color: modelColor }}>{model}</span>}
        {time && <span className="message__timestamp">{time}</span>}
        {(msg.items.length > 0 || msg.tool_call_count > 0 || msg.thinking_count > 0) && (
          <button
            className="message__detail-btn"
            onClick={(e) => { e.stopPropagation(); onOpenDetail(index); }}
          >
            Detail {"\u2192"}
          </button>
        )}
      </div>

      <div className={`message__content${!isExpanded ? " message__content--collapsed" : ""}`}>
        {contentPreview}
      </div>

      {hasStats && (
        <div className="message__stats">
          {msg.tokens_raw > 0 && (
            <span className={`message__stat${msg.tokens_raw > 150000 ? " message__stat--tokens-high" : ""}`}>
              <span className="message__stat-icon">{"\u{1FA99}"}</span>
              {formatTokens(msg.tokens_raw)} tok
            </span>
          )}
          {msg.tool_call_count > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u{1F527}"}</span>
              {msg.tool_call_count} tool{msg.tool_call_count > 1 ? "s" : ""}
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
          {subagentCount > 0 && (
            <span className="message__stat message__stat--agents">
              <span className="message__stat-icon">{"\u{1F9E9}"}</span>
              {subagentCount} agent{subagentCount > 1 ? "s" : ""}
            </span>
          )}
          {msg.duration_ms > 0 && (
            <span className="message__stat">
              <span className="message__stat-icon">{"\u23F1"}</span>
              {formatDuration(msg.duration_ms)}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

/* Agent detail view — same structure as main MessageDetail but inside panel */
function AgentDetailView({ msg, onBack }: { msg: DisplayMessage; onBack: () => void }) {
  const { set: expandedItems, toggle: toggleItem } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const [nestedAgent, setNestedAgent] = useState<DisplayItem | null>(null);

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);
  const hasItems = msg.items.length > 0;

  const handleItemClick = (index: number, item: DisplayItem) => {
    setSelectedItem(index);
    if (item.item_type === "Subagent" && item.subagent_messages.length > 0) {
      setNestedAgent(nestedAgent?.agent_id === item.agent_id ? null : item);
    } else {
      toggleItem(index);
    }
  };

  return (
    <div className={`agent-detail${nestedAgent ? " agent-detail--split" : ""}`}>
      <div className="agent-detail__main">
        <div className="message-detail__header">
          <BackButton onClick={onBack} />
          <span className="message-detail__title">
            {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
          </span>
          {model && <span style={{ color: modelColor, fontWeight: 600, fontSize: 12 }}>{model}</span>}
          <span className="message-detail__meta">
            {time}
            {msg.tokens_raw > 0 && <>{" "}{"\u00B7"} {formatTokens(msg.tokens_raw)} tok</>}
            {msg.duration_ms > 0 && <>{" "}{"\u00B7"} {formatDuration(msg.duration_ms)}</>}
          </span>
        </div>

        <div className="message-detail__body">
          <div className="message-detail__content">
            {msg.content && (
              <div className="message-detail__text">
                <ReactMarkdown>{msg.content}</ReactMarkdown>
              </div>
            )}
            {hasItems && (
              <div className="detail-items">
                <div className="detail-items__section-label">Items ({msg.items.length})</div>
                {msg.items.map((item, idx) => (
                  <DetailItem
                    key={idx}
                    ref={idx === selectedItem ? scrollRef : undefined}
                    item={item}
                    index={idx}
                    isSelected={idx === selectedItem}
                    isExpanded={item.item_type !== "Subagent" && expandedItems.has(idx)}
                    isAgentActive={nestedAgent?.agent_id === item.agent_id && item.item_type === "Subagent" && !!item.agent_id}
                    onToggle={handleItemClick}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {nestedAgent && (
        <AgentPanel item={nestedAgent} onClose={() => setNestedAgent(null)} />
      )}
    </div>
  );
}

/* ─── Shared detail item components ─── */

interface DetailItemProps {
  item: DisplayItem;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  isAgentActive?: boolean;
  onToggle: (index: number, item: DisplayItem) => void;
  onSelect: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

function DetailItem({
  ref, item, index, isSelected, isExpanded, isAgentActive, onToggle, onSelect,
}: DetailItemProps) {
  const icon = getItemIcon(item);
  const name = getItemName(item);
  const summary = getItemSummary(item);
  const teamClr = item.team_color ? getTeamColor(item.team_color) : undefined;
  const hasAgentMessages = item.item_type === "Subagent" && item.subagent_messages.length > 0;
  const [popout, setPopout] = useState(false);

  return (
    <div
      ref={ref}
      className={`detail-item${isSelected ? " detail-item--selected" : ""}${item.tool_error ? " detail-item--error" : ""}${isAgentActive ? " detail-item--agent-active" : ""}`}
    >
      <div
        className="detail-item__header"
        onClick={() => { onSelect(index); onToggle(index, item); }}
      >
        <span className={`detail-item__chevron${isExpanded ? " detail-item__chevron--expanded" : ""}${hasAgentMessages ? " detail-item__chevron--panel" : ""}`}>
          {hasAgentMessages ? "\u25A8" : "\u25B6"}
        </span>
        <span className="detail-item__icon">{icon}</span>
        <span className="detail-item__name" style={teamClr ? { color: teamClr } : undefined}>{name}</span>
        <span className="detail-item__summary">{summary}</span>
        {item.agent_id && <span className="detail-item__agent-id">{item.agent_id}</span>}
        <span className="detail-item__right">
          {item.duration_ms > 0 && <span className="detail-item__duration">{formatDuration(item.duration_ms)}</span>}
          {item.token_count > 0 && <span className="detail-item__tokens">{formatTokens(item.token_count)} tok</span>}
          {item.subagent_ongoing && <span className="detail-item__ongoing-dot" />}
          {isExpanded && (
            <button
              className="detail-item__popout-btn"
              onClick={(e) => { e.stopPropagation(); setPopout(true); }}
              title="Pop out to larger view"
            >
              {"\u2197"}
            </button>
          )}
        </span>
      </div>
      {isExpanded && <DetailItemBody item={item} />}
      {popout && (
        <PopoutModal onClose={() => setPopout(false)} header={
          <>
            <span className="popout-modal__icon">{icon}</span>
            <span className="popout-modal__name">{name}</span>
            {item.tool_summary && <span className="popout-modal__summary">{item.tool_summary}</span>}
            {item.agent_id && <span className="popout-modal__agent-id">{item.agent_id}</span>}
            <span className="popout-modal__right">
              {item.duration_ms > 0 && <span>{formatDuration(item.duration_ms)}</span>}
              {item.token_count > 0 && <span>{formatTokens(item.token_count)} tok</span>}
            </span>
          </>
        }>
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
          <div className="detail-item__text detail-item__text--thinking">{item.text}</div>
        </div>
      );
    case "Output":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text"><ReactMarkdown>{item.text}</ReactMarkdown></div>
        </div>
      );
    case "ToolCall":
      return (
        <div className="detail-item__body">
          {item.tool_input && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Input</div>
              <div className="detail-item__json"><pre><code>{formatJson(item.tool_input)}</code></pre></div>
            </div>
          )}
          {item.tool_result && (
            <div className="detail-item__section">
              <div className="detail-item__section-title">Result</div>
              <div className={`detail-item__text${item.tool_error ? " detail-item__text--error" : ""}`}>{item.tool_result}</div>
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
    default:
      return (
        <div className="detail-item__body">
          <div className="detail-item__text">{item.text}</div>
        </div>
      );
  }
}

function getItemIcon(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking": return "\u{1F4A1}";
    case "Output": return "\u{1F4AC}";
    case "ToolCall": return item.tool_error ? "\u26A0" : (toolCategoryIcons[item.tool_category] ?? "\u{1F527}");
    case "Subagent": return "\u{1F916}";
    case "TeammateMessage": return "\u{1F916}";
    default: return "\u2022";
  }
}

function getItemName(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking": return "Thinking";
    case "Output": return "Output";
    case "ToolCall": return item.tool_name || "Tool";
    case "Subagent": return item.subagent_type || "Subagent";
    case "TeammateMessage": return item.team_member_name || "Teammate";
    default: return item.item_type;
  }
}

function getItemSummary(item: DisplayItem): string {
  switch (item.item_type) {
    case "ToolCall": return item.tool_summary || "";
    case "Subagent": return item.subagent_desc || "";
    case "TeammateMessage": return item.text ? item.text.slice(0, 100) : "";
    case "Thinking": return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    case "Output": return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    default: return "";
  }
}
