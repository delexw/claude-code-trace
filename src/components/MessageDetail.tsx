import { useState, useMemo, useCallback, useRef, useLayoutEffect } from "react";
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
import { useKeyboard } from "../hooks/useKeyboard";
import { BackButton } from "./BackButton";
import { PopoutModal } from "./PopoutModal";
import { ResizeHandle } from "./ResizeHandle";

/* ─── Panel stack types ─── */

type PanelEntry =
  | { kind: "agent-list"; item: DisplayItem; key: string }
  | { kind: "agent-detail"; item: DisplayItem; msg: DisplayMessage; key: string };

/** Imperative handle exposed by each navigable column */
interface ColumnNav {
  moveUp: () => void;
  moveDown: () => void;
  toggle: () => void;
  enter: () => void;
  itemCount: () => number;
}

interface MessageDetailProps {
  message: DisplayMessage;
  onBack: () => void;
}

export function MessageDetail({ message: msg, onBack }: MessageDetailProps) {
  const { set: expandedItems, toggle: toggleItem } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const [panelStack, setPanelStack] = useState<PanelEntry[]>([]);
  const [columnWidths, setColumnWidths] = useState<(number | null)[]>([null]);
  const [focusedColumn, setFocusedColumn] = useState(0); // 0 = main, 1+ = panel index + 1
  const bodyRef = useRef<HTMLDivElement>(null);
  const savedScroll = useRef<number | null>(null);
  const panelRefs = useRef<Map<number, ColumnNav>>(new Map());

  useLayoutEffect(() => {
    if (savedScroll.current != null && bodyRef.current) {
      bodyRef.current.scrollTop = savedScroll.current;
      savedScroll.current = null;
    }
  }, [panelStack.length]);

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);
  const hasItems = msg.items.length > 0;
  const hasPanels = panelStack.length > 0;

  // Stack manipulation
  const openSubagentFromMain = useCallback((item: DisplayItem) => {
    if (bodyRef.current) {
      savedScroll.current = bodyRef.current.scrollTop;
    }
    setPanelStack((prev) => {
      if (
        prev.length === 1 &&
        prev[0].kind === "agent-list" &&
        prev[0].item.agent_id === item.agent_id
      ) {
        return [];
      }
      return [{ kind: "agent-list", item, key: item.agent_id || `panel-0` }];
    });
    setColumnWidths((prev) => [prev[0]]);
    setFocusedColumn(1);
  }, []);

  const openSubagentAt = useCallback((depth: number, item: DisplayItem) => {
    setPanelStack((prev) => {
      const next = prev.slice(0, depth + 1);
      next.push({ kind: "agent-list", item, key: item.agent_id || `panel-${depth + 1}` });
      return next;
    });
    setColumnWidths((prev) => prev.slice(0, depth + 2));
    setFocusedColumn(depth + 2);
  }, []);

  const openDetailAt = useCallback((depth: number, detailMsg: DisplayMessage) => {
    setPanelStack((prev) => {
      const entry = prev[depth];
      if (!entry || entry.kind !== "agent-list") return prev;
      const next = prev.slice(0, depth);
      next.push({
        kind: "agent-detail",
        item: entry.item,
        msg: detailMsg,
        key: entry.key + ":detail",
      });
      return next;
    });
  }, []);

  const backToListAt = useCallback((depth: number) => {
    setPanelStack((prev) => {
      const entry = prev[depth];
      if (!entry || entry.kind !== "agent-detail") return prev;
      const next = prev.slice(0, depth);
      next.push({
        kind: "agent-list",
        item: entry.item,
        key: entry.item.agent_id || `panel-${depth}`,
      });
      return next;
    });
  }, []);

  const closeAt = useCallback((depth: number) => {
    setPanelStack((prev) => prev.slice(0, depth));
    setColumnWidths((prev) => prev.slice(0, depth + 1));
    setFocusedColumn((prev) => Math.min(prev, depth));
  }, []);

  const setColumnWidth = useCallback((colIndex: number, width: number) => {
    setColumnWidths((prev) => {
      const next = [...prev];
      while (next.length <= colIndex) next.push(null);
      next[colIndex] = width;
      return next;
    });
  }, []);

  const handleItemClick = (index: number, item: DisplayItem) => {
    setSelectedItem(index);
    setFocusedColumn(0);
    if (item.subagent_messages.length > 0) {
      openSubagentFromMain(item);
    } else {
      toggleItem(index);
    }
  };

  const registerPanelNav = useCallback((depth: number, nav: ColumnNav | null) => {
    if (nav) {
      panelRefs.current.set(depth, nav);
    } else {
      panelRefs.current.delete(depth);
    }
  }, []);

  // Single keyboard handler for all columns
  const detailKeyMap: Record<string, () => void> = {};

  // Navigation: j/k/ArrowDown/ArrowUp/Tab/Enter dispatch to the focused column
  const moveDown = () => {
    if (focusedColumn === 0) {
      setSelectedItem((i) => Math.min(i + 1, msg.items.length - 1));
    } else {
      const nav = panelRefs.current.get(focusedColumn - 1);
      nav?.moveDown();
    }
  };
  const moveUp = () => {
    if (focusedColumn === 0) {
      setSelectedItem((i) => Math.max(i - 1, 0));
    } else {
      const nav = panelRefs.current.get(focusedColumn - 1);
      nav?.moveUp();
    }
  };
  detailKeyMap["j"] = moveDown;
  detailKeyMap["ArrowDown"] = moveDown;
  detailKeyMap["k"] = moveUp;
  detailKeyMap["ArrowUp"] = moveUp;
  detailKeyMap["Tab"] = () => {
    if (focusedColumn === 0) {
      const item = msg.items[selectedItem];
      if (item) handleItemClick(selectedItem, item);
    } else {
      const nav = panelRefs.current.get(focusedColumn - 1);
      nav?.toggle();
    }
  };
  detailKeyMap["Enter"] = () => {
    if (focusedColumn === 0) {
      const item = msg.items[selectedItem];
      if (item && item.subagent_messages.length > 0) {
        openSubagentFromMain(item);
      } else if (item) {
        toggleItem(selectedItem);
      }
    } else {
      const nav = panelRefs.current.get(focusedColumn - 1);
      nav?.enter();
    }
  };

  // h/l or ArrowLeft/ArrowRight to switch focused column
  detailKeyMap["h"] = () => setFocusedColumn((c) => Math.max(0, c - 1));
  detailKeyMap["ArrowLeft"] = () => setFocusedColumn((c) => Math.max(0, c - 1));
  detailKeyMap["l"] = () => setFocusedColumn((c) => Math.min(panelStack.length, c + 1));
  detailKeyMap["ArrowRight"] = () => setFocusedColumn((c) => Math.min(panelStack.length, c + 1));

  // q = go back to list
  detailKeyMap["q"] = onBack;
  // Escape = close rightmost panel, or go back
  detailKeyMap["Escape"] = () => {
    if (panelStack.length > 0) {
      closeAt(panelStack.length - 1);
    } else {
      onBack();
    }
  };

  useKeyboard(detailKeyMap);

  const mainWidthStyle =
    hasPanels && columnWidths[0]
      ? { flex: `0 0 ${columnWidths[0]}px`, maxWidth: columnWidths[0] }
      : undefined;

  return (
    <div className={`message-detail${hasPanels ? " message-detail--split" : ""}`}>
      <div
        className={`message-detail__main${focusedColumn === 0 ? " message-detail__main--focused" : ""}`}
        style={mainWidthStyle}
        onClick={() => setFocusedColumn(0)}
      >
        <div className="message-detail__header">
          <BackButton onClick={onBack} />
          <span className="message-detail__title">
            {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
          </span>
          {model && (
            <span style={{ color: modelColor, fontWeight: 600, fontSize: 12 }}>{model}</span>
          )}
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

        <div className="message-detail__body" ref={bodyRef}>
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
                    isAgentActive={
                      panelStack.length > 0 &&
                      panelStack[0].item.agent_id === item.agent_id &&
                      !!item.agent_id
                    }
                    onToggle={handleItemClick}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {panelStack.map((entry, depth) => {
        const colWidth = columnWidths[depth + 1];
        const widthStyle = colWidth ? { flex: `0 0 ${colWidth}px`, maxWidth: colWidth } : undefined;
        const isFocused = focusedColumn === depth + 1;

        return (
          <PanelColumn
            key={entry.key}
            entry={entry}
            depth={depth}
            style={widthStyle}
            isFocused={isFocused}
            panelStack={panelStack}
            onOpenDetail={(detailMsg) => openDetailAt(depth, detailMsg)}
            onBackToList={() => backToListAt(depth)}
            onOpenSubagent={(item) => openSubagentAt(depth, item)}
            onClose={() => closeAt(depth)}
            onResize={(w) => setColumnWidth(depth, w)}
            onFocus={() => setFocusedColumn(depth + 1)}
            onRegisterNav={(nav) => registerPanelNav(depth, nav)}
          />
        );
      })}
    </div>
  );
}

/* ─── PanelColumn: renders ResizeHandle + either list or detail ─── */

interface PanelColumnProps {
  entry: PanelEntry;
  depth: number;
  style?: React.CSSProperties;
  isFocused: boolean;
  panelStack: PanelEntry[];
  onOpenDetail: (msg: DisplayMessage) => void;
  onBackToList: () => void;
  onOpenSubagent: (item: DisplayItem) => void;
  onClose: () => void;
  onResize: (width: number) => void;
  onFocus: () => void;
  onRegisterNav: (nav: ColumnNav | null) => void;
}

function PanelColumn({
  entry,
  depth,
  style,
  isFocused,
  panelStack,
  onOpenDetail,
  onBackToList,
  onOpenSubagent,
  onClose,
  onResize,
  onFocus,
  onRegisterNav,
}: PanelColumnProps) {
  return (
    <>
      <ResizeHandle onResize={onResize} />
      {entry.kind === "agent-list" ? (
        <AgentListColumn
          item={entry.item}
          style={style}
          isFocused={isFocused}
          onOpenDetail={onOpenDetail}
          onClose={onClose}
          onFocus={onFocus}
          onRegisterNav={onRegisterNav}
        />
      ) : (
        <AgentDetailColumn
          item={entry.item}
          msg={entry.msg}
          style={style}
          isFocused={isFocused}
          depth={depth}
          panelStack={panelStack}
          onOpenSubagent={onOpenSubagent}
          onBack={onBackToList}
          onClose={onClose}
          onFocus={onFocus}
          onRegisterNav={onRegisterNav}
        />
      )}
    </>
  );
}

/* ─── AgentListColumn: shows agent header + message list ─── */

interface AgentListColumnProps {
  item: DisplayItem;
  style?: React.CSSProperties;
  isFocused: boolean;
  onOpenDetail: (msg: DisplayMessage) => void;
  onClose: () => void;
  onFocus: () => void;
  onRegisterNav: (nav: ColumnNav | null) => void;
}

function AgentListColumn({
  item,
  style,
  isFocused,
  onOpenDetail,
  onClose,
  onFocus,
  onRegisterNav,
}: AgentListColumnProps) {
  const messages = item.subagent_messages;
  const [selectedMsg, setSelectedMsg] = useState(messages.length - 1);
  const { set: expandedSet, toggle: toggleMsg } = useToggleSet();
  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useScrollToSelected(selectedMsg);
  const panelColor = item.team_color ? getTeamColor(item.team_color) : undefined;

  const handleClick = useCallback(
    (index: number) => {
      onFocus();
      if (selectedMsg === index) {
        toggleMsg(index);
      } else {
        setSelectedMsg(index);
      }
    },
    [selectedMsg, toggleMsg, onFocus],
  );

  // Expose navigation to parent via callback ref
  const navRef = useRef<ColumnNav>({
    moveUp: () => setSelectedMsg((i) => Math.max(i - 1, 0)),
    moveDown: () => setSelectedMsg((i) => Math.min(i + 1, messages.length - 1)),
    toggle: () => toggleMsg(selectedMsg),
    enter: () => {
      if (messages[selectedMsg]) onOpenDetail(messages[selectedMsg]);
    },
    itemCount: () => messages.length,
  });
  navRef.current.moveUp = () => setSelectedMsg((i) => Math.max(i - 1, 0));
  navRef.current.moveDown = () => setSelectedMsg((i) => Math.min(i + 1, messages.length - 1));
  navRef.current.toggle = () => toggleMsg(selectedMsg);
  navRef.current.enter = () => {
    if (messages[selectedMsg]) onOpenDetail(messages[selectedMsg]);
  };
  navRef.current.itemCount = () => messages.length;

  // Register/unregister nav on mount/unmount
  useLayoutEffect(() => {
    onRegisterNav(navRef.current);
    return () => onRegisterNav(null);
  }, [onRegisterNav]);

  const reversed = useMemo(() => {
    const indices: number[] = [];
    for (let i = messages.length - 1; i >= 0; i--) indices.push(i);
    return indices;
  }, [messages.length]);

  return (
    <div
      className={`agent-panel${isFocused ? " agent-panel--focused" : ""}`}
      style={
        {
          ...(panelColor ? { background: `${panelColor}08` } : {}),
          ...style,
        } as React.CSSProperties
      }
      onClick={onFocus}
    >
      <AgentPanelHeader item={item} panelColor={panelColor} onClose={onClose} />
      <div className="agent-panel__content">
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
                onOpenDetail={(idx) => onOpenDetail(messages[idx])}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}

/* ─── AgentDetailColumn: shows a single message's detail items ─── */

interface AgentDetailColumnProps {
  item: DisplayItem;
  msg: DisplayMessage;
  style?: React.CSSProperties;
  isFocused: boolean;
  depth: number;
  panelStack: PanelEntry[];
  onOpenSubagent: (item: DisplayItem) => void;
  onBack: () => void;
  onClose: () => void;
  onFocus: () => void;
  onRegisterNav: (nav: ColumnNav | null) => void;
}

function AgentDetailColumn({
  item,
  msg,
  style,
  isFocused,
  depth,
  panelStack,
  onOpenSubagent,
  onBack,
  onClose,
  onFocus,
  onRegisterNav,
}: AgentDetailColumnProps) {
  const { set: expandedItems, toggle: toggleItem } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const panelColor = item.team_color ? getTeamColor(item.team_color) : undefined;

  const model = msg.model ? shortModel(msg.model) : "";
  const modelColor = msg.model ? getModelColor(msg.model) : undefined;
  const time = formatExactTime(msg.timestamp);
  const hasItems = msg.items.length > 0;

  // Check if a deeper panel is open for a given agent_id
  const activeAgentId = depth + 1 < panelStack.length ? panelStack[depth + 1].item.agent_id : null;

  const handleItemClick = (index: number, clickedItem: DisplayItem) => {
    onFocus();
    setSelectedItem(index);
    if (clickedItem.subagent_messages.length > 0) {
      onOpenSubagent(clickedItem);
    } else {
      toggleItem(index);
    }
  };

  // Expose navigation to parent via callback ref
  const navRef = useRef<ColumnNav>({
    moveUp: () => {},
    moveDown: () => {},
    toggle: () => {},
    enter: () => {},
    itemCount: () => msg.items.length,
  });
  navRef.current.moveUp = () => setSelectedItem((i) => Math.max(i - 1, 0));
  navRef.current.moveDown = () => setSelectedItem((i) => Math.min(i + 1, msg.items.length - 1));
  navRef.current.toggle = () => {
    const it = msg.items[selectedItem];
    if (it) handleItemClick(selectedItem, it);
  };
  navRef.current.enter = () => {
    const it = msg.items[selectedItem];
    if (it && it.subagent_messages.length > 0) {
      onOpenSubagent(it);
    } else if (it) {
      toggleItem(selectedItem);
    }
  };
  navRef.current.itemCount = () => msg.items.length;

  useLayoutEffect(() => {
    onRegisterNav(navRef.current);
    return () => onRegisterNav(null);
  }, [onRegisterNav]);

  return (
    <div
      className={`agent-panel${isFocused ? " agent-panel--focused" : ""}`}
      style={
        {
          ...(panelColor ? { background: `${panelColor}08` } : {}),
          ...style,
        } as React.CSSProperties
      }
      onClick={onFocus}
    >
      <AgentPanelHeader item={item} panelColor={panelColor} onClose={onClose} />
      <div className="agent-panel__content">
        <div className="message-detail__header" style={{ borderBottom: "1px solid var(--border)" }}>
          <BackButton onClick={onBack} />
          <span className="message-detail__title">
            {msg.role === "user" ? "User" : msg.role === "claude" ? "Claude" : "System"}
          </span>
          {model && (
            <span style={{ color: modelColor, fontWeight: 600, fontSize: 12 }}>{model}</span>
          )}
          <span className="message-detail__meta">
            {time}
            {msg.tokens_raw > 0 && (
              <>
                {" "}
                {"\u00B7"} {formatTokens(msg.tokens_raw)} tok
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
                {msg.items.map((di, idx) => (
                  <DetailItem
                    key={idx}
                    ref={idx === selectedItem ? scrollRef : undefined}
                    item={di}
                    index={idx}
                    isSelected={idx === selectedItem}
                    isExpanded={di.item_type !== "Subagent" && expandedItems.has(idx)}
                    isAgentActive={activeAgentId === di.agent_id && !!di.agent_id}
                    onToggle={handleItemClick}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─── Shared agent panel header ─── */

function AgentPanelHeader({
  item,
  panelColor,
  onClose,
}: {
  item: DisplayItem;
  panelColor?: string;
  onClose: () => void;
}) {
  return (
    <div
      className="agent-panel__header"
      style={panelColor ? { background: `${panelColor}10` } : undefined}
    >
      <button className="agent-panel__close" onClick={onClose}>
        {"\u2715"}
      </button>
      <span className="agent-panel__icon">{"\u{1F916}"}</span>
      <span className="agent-panel__type" style={panelColor ? { color: panelColor } : undefined}>
        {item.subagent_type || item.tool_name || "Subagent"}
      </span>
      {(item.subagent_desc || item.tool_summary) && (
        <span className="agent-panel__desc">{item.subagent_desc || item.tool_summary}</span>
      )}
      {item.agent_id && <span className="agent-panel__id">{item.agent_id}</span>}
      <span className="agent-panel__stats">
        {item.duration_ms > 0 && <span>{formatDuration(item.duration_ms)}</span>}
        {item.token_count > 0 && <span>{formatTokens(item.token_count)} tok</span>}
      </span>
    </div>
  );
}

/* ─── Agent message item ─── */

interface AgentMessageItemProps {
  msg: DisplayMessage;
  index: number;
  isSelected: boolean;
  isExpanded: boolean;
  onClick: (index: number) => void;
  onOpenDetail: (index: number) => void;
  ref?: React.Ref<HTMLDivElement>;
}

function AgentMessageItem({
  ref,
  msg,
  index,
  isSelected,
  isExpanded,
  onClick,
  onOpenDetail,
}: AgentMessageItemProps) {
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

  const subagentCount = msg.items.filter(
    (it) => it.item_type === "Subagent" || it.subagent_messages.length > 0,
  ).length;
  const hasStats =
    msg.tokens_raw > 0 ||
    msg.tool_call_count > 0 ||
    msg.thinking_count > 0 ||
    msg.duration_ms > 0 ||
    subagentCount > 0;

  return (
    <div
      ref={ref}
      className={`message ${roleClass}${isSelected ? " message--selected" : ""}`}
      onClick={() => onClick(index)}
      onDoubleClick={() => onOpenDetail(index)}
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
        {time && <span className="message__timestamp">{time}</span>}
        {(msg.items.length > 0 || msg.tool_call_count > 0 || msg.thinking_count > 0) && (
          <button
            className="message__detail-btn"
            onClick={(e) => {
              e.stopPropagation();
              onOpenDetail(index);
            }}
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
  ref,
  item,
  index,
  isSelected,
  isExpanded,
  isAgentActive,
  onToggle,
  onSelect,
}: DetailItemProps) {
  const icon = getItemIcon(item);
  const name = getItemName(item);
  const summary = getItemSummary(item);
  const teamClr = item.team_color ? getTeamColor(item.team_color) : undefined;
  const hasAgentMessages = item.subagent_messages.length > 0;
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
          {hasAgentMessages ? "\u25A8" : "\u25B6"}
        </span>
        <span className="detail-item__icon">{icon}</span>
        <span className="detail-item__name" style={teamClr ? { color: teamClr } : undefined}>
          {name}
        </span>
        <span className="detail-item__summary">{summary}</span>
        {item.agent_id && <span className="detail-item__agent-id">{item.agent_id}</span>}
        <span className="detail-item__right">
          {item.duration_ms > 0 && (
            <span className="detail-item__duration">{formatDuration(item.duration_ms)}</span>
          )}
          {item.token_count > 0 && (
            <span className="detail-item__tokens">{formatTokens(item.token_count)} tok</span>
          )}
          {item.subagent_ongoing && <span className="detail-item__ongoing-dot" />}
          {isExpanded && (
            <button
              className="detail-item__popout-btn"
              onClick={(e) => {
                e.stopPropagation();
                setPopout(true);
              }}
              title="Pop out to larger view"
            >
              {"\u2197"}
            </button>
          )}
        </span>
      </div>
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
                {item.token_count > 0 && <span>{formatTokens(item.token_count)} tok</span>}
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
    case "Thinking":
      return "\u{1F4A1}";
    case "Output":
      return "\u{1F4AC}";
    case "ToolCall":
      return item.tool_error ? "\u26A0" : (toolCategoryIcons[item.tool_category] ?? "\u{1F527}");
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
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "Content not recorded";
    case "Output":
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "\u2026" : "") : "";
    default:
      return "";
  }
}
