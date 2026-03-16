import { useState, useMemo, useCallback, useRef, useLayoutEffect, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import type { DisplayMessage, DisplayItem } from "../types";
import { shortModel, formatTokens, formatDuration, formatExactTime } from "../lib/format";
import { getModelColor, getTeamColor } from "../lib/theme";
import { MessageItem } from "./MessageItem";
import { DetailItem } from "./DetailItem";
import { useToggleSet } from "../hooks/useToggleSet";
import { useScrollToSelected } from "../hooks/useScrollToSelected";
import { useAutoScroll } from "../hooks/useAutoScroll";
import { useKeyboard } from "../hooks/useKeyboard";
import { useRegisterViewActions, type ViewActionsRef } from "../hooks/useViewActions";
import { BackButton } from "./BackButton";
import { ResizeHandle } from "./ResizeHandle";
import { OngoingDots } from "./OngoingDots";
import { IoMdCloseCircle } from "react-icons/io";
import { ClaudeIcon } from "./Icons";

/* ─── Helpers ─── */

/** Recursively find a DisplayItem by agent_id in a nested item tree. */
function findItemByAgentId(items: DisplayItem[], agentId: string): DisplayItem | undefined {
  for (const item of items) {
    if (item.agent_id === agentId) return item;
    for (const subMsg of item.subagent_messages) {
      const found = findItemByAgentId(subMsg.items, agentId);
      if (found) return found;
    }
  }
  return undefined;
}

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
  expandAll: () => void;
  collapseAll: () => void;
}

interface MessageDetailProps {
  message: DisplayMessage;
  ongoing?: boolean;
  onBack: () => void;
  viewActionsRef: ViewActionsRef;
}

export function MessageDetail({
  message: msg,
  ongoing,
  onBack,
  viewActionsRef,
}: MessageDetailProps) {
  const {
    set: expandedItems,
    toggle: toggleItem,
    addAll: expandAllItems,
    clear: clearItems,
  } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const [panelStack, setPanelStack] = useState<PanelEntry[]>([]);
  const [columnWidths, setColumnWidths] = useState<(number | null)[]>([null]);
  const [focusedColumn, setFocusedColumn] = useState(0); // 0 = main, 1+ = panel index + 1
  const bodyRef = useRef<HTMLDivElement>(null);
  useAutoScroll(msg.items.length, bodyRef);
  const savedScroll = useRef<number | null>(null);
  const panelRefs = useRef<Map<number, ColumnNav>>(new Map());

  const detailExpandAll = useCallback(() => {
    if (focusedColumn === 0) {
      expandAllItems(msg.items.map((_, i) => i));
    } else {
      panelRefs.current.get(focusedColumn - 1)?.expandAll();
    }
  }, [msg.items, expandAllItems, focusedColumn]);

  const detailCollapseAll = useCallback(() => {
    if (focusedColumn === 0) {
      clearItems();
    } else {
      panelRefs.current.get(focusedColumn - 1)?.collapseAll();
    }
  }, [clearItems, focusedColumn]);

  useRegisterViewActions(viewActionsRef, {
    expandAll: detailExpandAll,
    collapseAll: detailCollapseAll,
  });

  // Keep panel stack items fresh when the session watcher sends updated data.
  // Without this, subagent_ongoing and subagent_messages would be stale.
  // Only updates state when something meaningful changed to avoid extra re-renders.
  useEffect(() => {
    if (panelStack.length === 0) return;
    setPanelStack((prev) => {
      let changed = false;
      const next = prev.map((entry) => {
        if (!entry.item.agent_id) return entry;
        const fresh = findItemByAgentId(msg.items, entry.item.agent_id);
        if (!fresh) return entry;
        // Skip if nothing meaningful changed.
        if (
          fresh.subagent_ongoing === entry.item.subagent_ongoing &&
          fresh.subagent_messages.length === entry.item.subagent_messages.length
        ) {
          return entry;
        }
        changed = true;
        if (entry.kind === "agent-list") {
          return { ...entry, item: fresh };
        }
        // For agent-detail, also refresh the message by matching timestamp.
        const freshMsg =
          fresh.subagent_messages.find(
            (m) => m.timestamp === entry.msg.timestamp && m.role === entry.msg.role,
          ) ?? entry.msg;
        return { ...entry, item: fresh, msg: freshMsg };
      });
      return changed ? next : prev;
    });
  }, [msg.items, panelStack.length]);

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
                    isExpanded={expandedItems.has(idx)}
                    isAgentActive={
                      panelStack.length > 0 &&
                      panelStack[0].item.agent_id === item.agent_id &&
                      !!item.agent_id
                    }
                    onToggle={handleItemClick}
                    onToggleExpand={toggleItem}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
            {ongoing && (
              <div className="message-detail__ongoing">
                <OngoingDots />
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
  const {
    set: expandedSet,
    toggle: toggleMsg,
    addAll: expandAllMsgs,
    clear: clearMsgs,
  } = useToggleSet();
  const listRef = useAutoScroll<HTMLDivElement>(messages.length);
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
    expandAll: () => expandAllMsgs(messages.map((_, i) => i)),
    collapseAll: () => clearMsgs(),
  });
  navRef.current.moveUp = () => setSelectedMsg((i) => Math.max(i - 1, 0));
  navRef.current.moveDown = () => setSelectedMsg((i) => Math.min(i + 1, messages.length - 1));
  navRef.current.toggle = () => toggleMsg(selectedMsg);
  navRef.current.enter = () => {
    if (messages[selectedMsg]) onOpenDetail(messages[selectedMsg]);
  };
  navRef.current.itemCount = () => messages.length;
  navRef.current.expandAll = () => expandAllMsgs(messages.map((_, i) => i));
  navRef.current.collapseAll = () => clearMsgs();

  // Register/unregister nav on mount/unmount
  useLayoutEffect(() => {
    onRegisterNav(navRef.current);
    return () => onRegisterNav(null);
  }, [onRegisterNav]);

  const ordered = useMemo(() => {
    const indices: number[] = [];
    for (let i = 0; i < messages.length; i++) indices.push(i);
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
          {ordered.map((i) => {
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
            const isLast = i === messages.length - 1;
            return (
              <MessageItem
                key={i}
                ref={isSelected ? selectedRef : undefined}
                message={msg}
                index={i}
                isSelected={isSelected}
                isExpanded={isExpanded}
                isOngoing={isLast && item.subagent_ongoing}
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
  const {
    set: expandedItems,
    toggle: toggleItem,
    addAll: expandAllDetailItems,
    clear: clearDetailItems,
  } = useToggleSet();
  const [selectedItem, setSelectedItem] = useState(0);
  const scrollRef = useScrollToSelected(selectedItem);
  const detailBodyRef = useAutoScroll<HTMLDivElement>(msg.items.length);
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
    expandAll: () => expandAllDetailItems(msg.items.map((_, i) => i)),
    collapseAll: () => clearDetailItems(),
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
  navRef.current.expandAll = () => expandAllDetailItems(msg.items.map((_, i) => i));
  navRef.current.collapseAll = () => clearDetailItems();

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
        <div className="message-detail__body" ref={detailBodyRef}>
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
                    isExpanded={expandedItems.has(idx)}
                    isAgentActive={activeAgentId === di.agent_id && !!di.agent_id}
                    onToggle={handleItemClick}
                    onToggleExpand={toggleItem}
                    onSelect={setSelectedItem}
                  />
                ))}
              </div>
            )}
            {item.subagent_ongoing && (
              <div className="message-detail__ongoing">
                <OngoingDots />
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
        <IoMdCloseCircle className="icon--close" />
      </button>
      <span className="agent-panel__icon">
        <ClaudeIcon />
      </span>
      <span className="agent-panel__type" style={panelColor ? { color: panelColor } : undefined}>
        {item.subagent_type || item.tool_name || "Subagent"}
      </span>
      {(item.subagent_desc || item.tool_summary) && (
        <span className="agent-panel__desc">{item.subagent_desc || item.tool_summary}</span>
      )}
      {item.agent_id && <span className="agent-panel__id">{item.agent_id}</span>}
      <span className="agent-panel__stats">
        {item.duration_ms > 0 && <span>{formatDuration(item.duration_ms)}</span>}
      </span>
    </div>
  );
}
