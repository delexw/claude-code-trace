import { useState, useRef, useEffect, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import type { DisplayMessage, DisplayItem } from "../types";
import { shortModel, formatTokens, formatDuration, formatExactTime } from "../lib/format";
import { getModelColor, getTeamColor, toolCategoryIcons } from "../lib/theme";

interface MessageDetailProps {
  message: DisplayMessage;
  onBack: () => void;
}

export function MessageDetail({ message: msg, onBack }: MessageDetailProps) {
  const [expandedItems, setExpandedItems] = useState<Set<number>>(new Set());
  const [selectedItem, setSelectedItem] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);

  const toggleItem = useCallback((index: number) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }, []);

  // Scroll selected item into view
  const itemRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  useEffect(() => {
    itemRefs.current.get(selectedItem)?.scrollIntoView({ block: "nearest" });
  }, [selectedItem]);

  // Group items by type for section headers
  const hasItems = msg.items.length > 0;

  return (
    <div className="message-detail">
      <div className="message-detail__header">
        <button className="message-detail__back" onClick={onBack}>
          {"\u2190"} Back
        </button>
        <span className="message-detail__title">
          {msg.role === "user"
            ? "User"
            : msg.role === "claude"
              ? "Claude"
              : "System"}
        </span>
        {model && (
          <span style={{ color: modelColor, fontWeight: 600, fontSize: 12 }}>
            {model}
          </span>
        )}
        {msg.subagent_label && (
          <span className="detail-item__subagent-badge">
            {msg.subagent_label}
          </span>
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

      <div className="message-detail__body" ref={bodyRef}>
        <div className="message-detail__content">
          {/* Main content with markdown rendering */}
          {msg.content && (
            <div className="message-detail__text">
              <ReactMarkdown>{msg.content}</ReactMarkdown>
            </div>
          )}

          {/* Structured items */}
          {hasItems && (
            <div className="detail-items">
              <div className="detail-items__section-label">
                Items ({msg.items.length})
              </div>
              {msg.items.map((item, idx) => (
                <DetailItem
                  key={idx}
                  ref={(el) => {
                    if (el) itemRefs.current.set(idx, el);
                    else itemRefs.current.delete(idx);
                  }}
                  item={item}
                  index={idx}
                  isSelected={idx === selectedItem}
                  isExpanded={expandedItems.has(idx)}
                  onToggle={toggleItem}
                  onSelect={setSelectedItem}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface DetailItemProps {
  item: DisplayItem;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  onToggle: (index: number) => void;
  onSelect: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

function DetailItem({
  ref,
  item,
  index,
  isSelected,
  isExpanded,
  onToggle,
  onSelect,
}: DetailItemProps) {
  const icon = getItemIcon(item);
  const name = getItemName(item);
  const summary = getItemSummary(item);
  const teamClr = item.team_color ? getTeamColor(item.team_color) : undefined;

  return (
    <div
      ref={ref}
      className={`detail-item${isSelected ? " detail-item--selected" : ""}${item.tool_error ? " detail-item--error" : ""}`}
    >
      <div
        className="detail-item__header"
        onClick={() => {
          onSelect(index);
          onToggle(index);
        }}
      >
        <span
          className={`detail-item__chevron${isExpanded ? " detail-item__chevron--expanded" : ""}`}
        >
          {"\u25B6"}
        </span>
        <span className="detail-item__icon">{icon}</span>
        <span
          className="detail-item__name"
          style={teamClr ? { color: teamClr } : undefined}
        >
          {name}
        </span>
        <span className="detail-item__summary">{summary}</span>
        <span className="detail-item__right">
          {item.duration_ms > 0 && (
            <span className="detail-item__duration">
              {formatDuration(item.duration_ms)}
            </span>
          )}
          {item.token_count > 0 && (
            <span className="detail-item__tokens">
              {formatTokens(item.token_count)} tok
            </span>
          )}
          {item.subagent_ongoing && <span className="detail-item__ongoing-dot" />}
        </span>
      </div>

      {isExpanded && <DetailItemBody item={item} />}
    </div>
  );
}

function DetailItemBody({ item }: { item: DisplayItem }) {
  switch (item.item_type) {
    case "Thinking":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text detail-item__text--thinking">
            {item.text}
          </div>
        </div>
      );

    case "Output":
      return (
        <div className="detail-item__body">
          <div className="detail-item__text">
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
    case "Thinking":
      return "\u{1F4A1}";
    case "Output":
      return "\u{1F4AC}";
    case "ToolCall":
      if (item.tool_error) return "\u26A0";
      return toolCategoryIcons[item.tool_category] ?? "\u{1F527}";
    case "Subagent":
      return "\u{1F916}";
    case "TeammateMessage":
      return "\u{1F916}";
    default:
      return "\u2022";
  }
}

function getItemName(item: DisplayItem): string {
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
    default:
      return item.item_type;
  }
}

function getItemSummary(item: DisplayItem): string {
  switch (item.item_type) {
    case "ToolCall":
      return item.tool_summary || "";
    case "Subagent":
      return item.subagent_desc || "";
    case "TeammateMessage":
      return item.text ? item.text.slice(0, 100) : "";
    case "Thinking":
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    case "Output":
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    default:
      return "";
  }
}

function formatJson(input: string): string {
  try {
    return JSON.stringify(JSON.parse(input), null, 2);
  } catch {
    return input;
  }
}
