import { useCallback, useMemo, useState } from "react";
import { invoke } from "../../lib/invoke";
import type { DisplayMessage } from "../../types";
import type { KeyHint, SourceContext, SourceRuntime, SourceView } from "../types";
import { MessageList } from "../../components/MessageList";
import { MessageDetail } from "../../components/MessageDetail";
import { BackIcon } from "../../components/Icons";
import { BackToMessagesButton } from "../../components/BackToMessagesButton";
import { AntigravityInfoBar } from "./AntigravityInfoBar";
import { AntigravityPickerSection, visibleConversations } from "./AntigravityPickerSection";
import { roleStyle } from "./roleStyles";
import { toDisplayMessages } from "./toDisplay";
import type { AntigravityBrainFolder, AntigravityMessage, AntigravityWindow } from "./types";
import "./antigravity.css";

interface OpenConversation {
  folder: AntigravityBrainFolder;
  steps: AntigravityMessage[];
  messages: DisplayMessage[];
  /** Every step in the transcript, including ones that recorded nothing. */
  totalRows: number;
}

const LIST_HINTS: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "G/g", label: "jump" },
  { key: "Tab", label: "toggle" },
  { key: "Enter", label: "detail" },
  { key: "e/c", label: "expand/collapse" },
  { key: "q", label: "sessions" },
];

const DETAIL_HINTS: KeyHint[] = [
  { key: "j/k", label: "items" },
  { key: "Tab", label: "toggle" },
  { key: "Enter", label: "open" },
  { key: "q/Esc", label: "back" },
];

const NO_FOLDERS: AntigravityBrainFolder[] = [];

export function useAntigravitySource(ctx: SourceContext): SourceRuntime {
  const { openView, goToPicker, viewActionsRef, searchQuery } = ctx;
  const [folders, setFolders] = useState<AntigravityBrainFolder[]>(NO_FOLDERS);
  const [loading, setLoading] = useState(false);
  const [conversation, setConversation] = useState<OpenConversation | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set([0]));

  const loadFolders = useCallback(async () => {
    setLoading(true);
    try {
      setFolders(await invoke<AntigravityBrainFolder[]>("list_antigravity_folders"));
    } catch (err) {
      console.error("Failed to list Antigravity conversations:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  const conversations = useMemo(
    () => visibleConversations(folders, searchQuery),
    [folders, searchQuery],
  );

  const openConversation = useCallback(
    async (folder: AntigravityBrainFolder) => {
      try {
        const whole = await invoke<AntigravityWindow>("load_antigravity_session", {
          path: folder.path,
        });
        setConversation({
          folder,
          steps: whole.messages,
          messages: toDisplayMessages(whole.messages),
          totalRows: whole.total_rows,
        });
        setSelectedIndex(0);
        setExpanded(new Set([0]));
        openView("list");
      } catch (err) {
        console.error("Failed to open Antigravity conversation:", err);
      }
    },
    [openView],
  );

  const showList = useCallback(() => openView("list"), [openView]);
  const openDetail = useCallback(
    (index: number) => {
      setSelectedIndex(index);
      openView("detail");
    },
    [openView],
  );

  const count = conversation?.messages.length ?? 0;
  const getMessage = useCallback((index: number) => conversation?.messages[index], [conversation]);
  const roles = useMemo(
    () => conversation?.messages.map((message) => message.role) ?? [],
    [conversation],
  );
  const rowRoleStyle = useCallback(
    (index: number) => {
      const step = conversation?.steps[index];
      return step ? roleStyle(step.role) : undefined;
    },
    [conversation],
  );
  const toggle = useCallback((index: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);
  const expandAll = useCallback(
    () => setExpanded(new Set(Array.from({ length: count }, (_, i) => i))),
    [count],
  );
  const collapseAll = useCallback(() => setExpanded(new Set()), []);
  // The whole conversation is already loaded, so there is no window to fetch.
  const noWindow = useCallback(() => {}, []);

  const infoBar = conversation ? (
    <AntigravityInfoBar folder={conversation.folder} steps={count} />
  ) : undefined;

  const moveDown = () => setSelectedIndex((i) => Math.min(i + 1, count - 1));
  const moveUp = () => setSelectedIndex((i) => Math.max(i - 1, 0));

  const listView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: (
      <button className="view-toolbar__btn" onClick={goToPicker}>
        <BackIcon /> Sessions
      </button>
    ),
    keyMap: {
      j: moveDown,
      ArrowDown: moveDown,
      k: moveUp,
      ArrowUp: moveUp,
      G: () => setSelectedIndex(Math.max(count - 1, 0)),
      g: () => setSelectedIndex(0),
      Tab: () => toggle(selectedIndex),
      Enter: () => {
        if (count > 0) openDetail(selectedIndex);
      },
      e: () => viewActionsRef.current.expandAll?.(),
      c: () => viewActionsRef.current.collapseAll?.(),
      q: goToPicker,
      Escape: goToPicker,
      s: goToPicker,
    },
    hints: LIST_HINTS,
    hintActions: { sessions: goToPicker },
    render: () => {
      if (!conversation) return null;
      if (count === 0) {
        return (
          <div className="antigravity-empty">
            {conversation.totalRows === 0 ? (
              <>
                No transcript found at{" "}
                <code>{conversation.folder.path}/.system_generated/logs/transcript.jsonl</code>
              </>
            ) : (
              <>
                None of the {conversation.totalRows} steps in this conversation recorded any text.
              </>
            )}
          </div>
        );
      }
      return (
        <MessageList
          key={ctx.fontScale}
          count={count}
          getMessage={getMessage}
          roles={roles}
          selectedIndex={selectedIndex}
          expandedSet={expanded}
          ongoing={false}
          onRangeChange={noWindow}
          onSelect={setSelectedIndex}
          onToggle={toggle}
          onOpenDetail={openDetail}
          viewActionsRef={viewActionsRef}
          onExpandAll={expandAll}
          onCollapseAll={collapseAll}
          rowRoleStyle={rowRoleStyle}
        />
      );
    },
  };

  const detailStep = conversation?.steps[selectedIndex];
  const detailView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: <BackToMessagesButton onClick={showList} />,
    // j/k/Tab/Enter/q/Escape are handled by MessageDetail's own keyboard hook.
    keyMap: {},
    hints: DETAIL_HINTS,
    hintActions: { back: showList },
    render: () => {
      const message = conversation?.messages[selectedIndex];
      if (!message || !detailStep) return null;
      return (
        <MessageDetail
          message={message}
          onBack={showList}
          viewActionsRef={viewActionsRef}
          roleLabel={roleStyle(detailStep.role).label}
          debugHint={false}
          emptyNotice="This step records no text, tool call or result."
        />
      );
    },
  };

  return {
    picker: {
      count: conversations.length,
      open: (localIndex) => {
        const folder = conversations[localIndex];
        if (folder) void openConversation(folder);
      },
      render: ({ selectedIndex: pickerIndex, onHover }) => (
        <AntigravityPickerSection
          conversations={conversations}
          loading={loading}
          searchQuery={searchQuery}
          selectedIndex={pickerIndex}
          onHover={onHover}
          onOpen={(folder) => void openConversation(folder)}
        />
      ),
    },
    hasOpenSession: conversation !== null,
    backToSession: () => {
      if (conversation) showList();
    },
    views: { list: listView, detail: detailView },
    onSettingsLoaded: loadFolders,
    onSettingsSaved: () => void loadFolders(),
  };
}
