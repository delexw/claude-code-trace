import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "../../lib/invoke";
import type { DisplayMessage } from "../../types";
import type { KeyHint, SourceContext, SourceRuntime, SourceView } from "../types";
import { MessageList } from "../../components/MessageList";
import { MessageDetail } from "../../components/MessageDetail";
import { BackIcon } from "../../components/Icons";
import { BackToMessagesButton } from "../../components/BackToMessagesButton";
import { CodexInfoBar } from "./CodexInfoBar";
import { CodexPickerSection, visibleSessions } from "./CodexPickerSection";
import { roleLabel, roleStyle } from "./roleStyles";
import type { CodexSession, CodexWindow } from "./types";
import "./codex.css";

interface OpenChat {
  session: CodexSession;
  window: CodexWindow;
  messages: Map<number, DisplayMessage>;
}

const WINDOW_SIZE = 80;
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

function messageMap(window: CodexWindow): Map<number, DisplayMessage> {
  return new Map(window.messages.map((message, offset) => [window.start + offset, message]));
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useCodexSource(ctx: SourceContext): SourceRuntime {
  const { openView, goToPicker, viewActionsRef } = ctx;
  const [sessions, setSessions] = useState<CodexSession[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [chat, setChat] = useState<OpenChat | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const enabledRef = useRef(ctx.enabled);
  const chatRef = useRef(chat);
  const epoch = useRef(0);
  const listRequest = useRef(0);
  const visibleRange = useRef({ start: 0, end: WINDOW_SIZE });
  const pendingWindows = useRef(new Set<string>());

  useLayoutEffect(() => {
    if (!ctx.enabled && enabledRef.current) {
      epoch.current++;
      listRequest.current++;
    }
    enabledRef.current = ctx.enabled;
    chatRef.current = chat;
  });
  useEffect(
    () => () => {
      epoch.current++;
      listRequest.current++;
    },
    [],
  );
  const loadSessions = useCallback(async () => {
    const request = ++listRequest.current;
    setLoading(true);
    try {
      const found = await invoke<CodexSession[]>("list_codex_sessions");
      if (request !== listRequest.current || !enabledRef.current) return;
      setSessions(found);
      setError(null);
    } catch (failure) {
      if (request === listRequest.current && enabledRef.current)
        setError(`Could not read Codex chats: ${errorText(failure)}`);
    } finally {
      if (request === listRequest.current) setLoading(false);
    }
  }, []);

  const openChat = useCallback(
    async (session: CodexSession) => {
      if (!enabledRef.current) return;
      const request = ++epoch.current;
      setError(null);
      try {
        const index = await invoke<CodexWindow>("load_codex_session", {
          path: session.path,
          limit: 0,
        });
        if (request !== epoch.current || !enabledRef.current) return;
        const start = Math.max(0, index.count - WINDOW_SIZE);
        const window =
          index.count === 0
            ? index
            : await invoke<CodexWindow>("load_codex_session", {
                path: session.path,
                start,
                limit: WINDOW_SIZE,
              });
        if (request !== epoch.current || !enabledRef.current) return;
        visibleRange.current = { start, end: window.count };
        setChat({ session, window, messages: messageMap(window) });
        setSelectedIndex(Math.max(0, window.count - 1));
        setExpanded(new Set([Math.max(0, window.count - 1)]));
        openView("list");
      } catch (failure) {
        if (request === epoch.current && enabledRef.current)
          setError(`Could not open Codex chat: ${errorText(failure)}`);
      }
    },
    [openView],
  );

  const ensureRange = useCallback(async (start: number, end: number) => {
    const current = chatRef.current;
    if (!current || !enabledRef.current) return;
    const from = Math.max(0, start - 10);
    const to = Math.min(current.window.count, Math.max(end + 10, from + WINDOW_SIZE));
    if (
      Array.from({ length: Math.max(0, to - from) }, (_, offset) => from + offset).every((index) =>
        current.messages.has(index),
      )
    )
      return;
    const request = epoch.current;
    const key = `${request}:${current.window.revision}:${from}:${to}`;
    if (pendingWindows.current.has(key)) return;
    pendingWindows.current.add(key);
    try {
      const window = await invoke<CodexWindow>("load_codex_session", {
        path: current.session.path,
        start: from,
        limit: to - from,
      });
      if (request !== epoch.current || !enabledRef.current) return;
      setChat((latest) => {
        if (
          !latest ||
          latest.session.path !== current.session.path ||
          latest.window.revision !== window.revision
        )
          return latest;
        return { ...latest, messages: new Map([...latest.messages, ...messageMap(window)]) };
      });
    } catch (failure) {
      if (request === epoch.current && enabledRef.current)
        setError(`Could not load Codex messages: ${errorText(failure)}`);
    } finally {
      pendingWindows.current.delete(key);
    }
  }, []);

  const path = chat?.session.path;
  useEffect(() => {
    if (!ctx.enabled || ctx.activeView === null || !path) return;
    let stopped = false;
    let busy = false;
    const timer = window.setInterval(async () => {
      if (busy) return;
      busy = true;
      const request = epoch.current;
      try {
        const index = await invoke<CodexWindow>("load_codex_session", { path, limit: 0 });
        const current = chatRef.current;
        if (
          stopped ||
          request !== epoch.current ||
          !enabledRef.current ||
          !current ||
          current.session.path !== path
        )
          return;
        if (current.window.revision === index.revision) {
          if (current.window.ongoing !== index.ongoing) {
            setChat((latest) =>
              latest?.window.revision === index.revision
                ? { ...latest, window: { ...latest.window, ongoing: index.ongoing } }
                : latest,
            );
          }
          setError(null);
          return;
        }
        const start = Math.max(0, Math.min(visibleRange.current.start, index.count - WINDOW_SIZE));
        const limit = Math.max(WINDOW_SIZE, visibleRange.current.end - start);
        const updated =
          index.count === 0
            ? index
            : await invoke<CodexWindow>("load_codex_session", { path, start, limit });
        if (stopped || request !== epoch.current || !enabledRef.current) return;
        setChat({ session: current.session, window: updated, messages: messageMap(updated) });
        setSelectedIndex((selected) => Math.min(selected, Math.max(0, updated.count - 1)));
        setError(null);
      } catch (failure) {
        if (!stopped && request === epoch.current && enabledRef.current)
          setError(`Could not refresh Codex chat: ${errorText(failure)}`);
      } finally {
        busy = false;
      }
    }, 2000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [ctx.enabled, ctx.activeView, path]);

  useEffect(() => {
    if (!ctx.enabled || ctx.activeView !== null) return;
    const timer = window.setInterval(() => void loadSessions(), 10000);
    return () => window.clearInterval(timer);
  }, [ctx.enabled, ctx.activeView, loadSessions]);

  const conversations = useMemo(
    () => visibleSessions(sessions, ctx.searchQuery),
    [sessions, ctx.searchQuery],
  );
  const count = chat?.window.count ?? 0;
  const showList = useCallback(() => openView("list"), [openView]);
  const openDetail = useCallback(
    (index: number) => {
      if (index < 0 || index >= (chatRef.current?.window.count ?? 0)) return;
      setSelectedIndex(index);
      visibleRange.current = { start: index, end: index + 1 };
      void ensureRange(index, index + 1);
      openView("detail");
    },
    [ensureRange, openView],
  );
  const toggle = useCallback(
    (index: number) =>
      setExpanded((previous) => {
        const next = new Set(previous);
        if (next.has(index)) next.delete(index);
        else next.add(index);
        return next;
      }),
    [],
  );
  const expandAll = useCallback(
    () => setExpanded(new Set(Array.from({ length: count }, (_, index) => index))),
    [count],
  );
  const collapseAll = useCallback(() => setExpanded(new Set()), []);
  const getMessage = useCallback((index: number) => chat?.messages.get(index), [chat]);
  const rowRoleStyle = useCallback((index: number) => roleStyle(chat?.messages.get(index)), [chat]);
  const onRangeChange = useCallback(
    (start: number, end: number) => {
      visibleRange.current = { start, end };
      void ensureRange(start, end);
    },
    [ensureRange],
  );
  const infoBar = chat ? <CodexInfoBar session={chat.session} window={chat.window} /> : undefined;
  const alert = error ? (
    <div className="codex-error" role="alert">
      {error}
    </div>
  ) : null;

  const listView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: (
      <button className="view-toolbar__btn" onClick={goToPicker}>
        <BackIcon /> Sessions
      </button>
    ),
    keyMap: {
      j: () => setSelectedIndex((index) => Math.min(index + 1, Math.max(0, count - 1))),
      ArrowDown: () => setSelectedIndex((index) => Math.min(index + 1, Math.max(0, count - 1))),
      k: () => setSelectedIndex((index) => Math.max(index - 1, 0)),
      ArrowUp: () => setSelectedIndex((index) => Math.max(index - 1, 0)),
      G: () => setSelectedIndex(Math.max(0, count - 1)),
      g: () => setSelectedIndex(0),
      Tab: () => {
        if (count > 0) toggle(selectedIndex);
      },
      Enter: () => openDetail(selectedIndex),
      e: () => viewActionsRef.current.expandAll?.(),
      c: () => viewActionsRef.current.collapseAll?.(),
      q: goToPicker,
      Escape: goToPicker,
      s: goToPicker,
    },
    hints: LIST_HINTS,
    hintActions: { sessions: goToPicker },
    render: () => (
      <>
        {alert}
        {!chat ? null : count === 0 ? (
          <div className="codex-empty">This Codex chat has no recorded messages yet.</div>
        ) : (
          <MessageList
            key={`${path}:${ctx.fontScale}`}
            count={count}
            getMessage={getMessage}
            roles={chat.window.roles}
            selectedIndex={selectedIndex}
            expandedSet={expanded}
            ongoing={chat.window.ongoing}
            onRangeChange={onRangeChange}
            onSelect={setSelectedIndex}
            onToggle={toggle}
            onOpenDetail={openDetail}
            viewActionsRef={viewActionsRef}
            onExpandAll={expandAll}
            onCollapseAll={collapseAll}
            rowRoleStyle={rowRoleStyle}
          />
        )}
      </>
    ),
  };

  const detailView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: <BackToMessagesButton onClick={showList} />,
    keyMap: {},
    hints: DETAIL_HINTS,
    hintActions: { back: showList },
    render: () => {
      const message = chat?.messages.get(selectedIndex);
      return (
        <>
          {alert}
          {message ? (
            <MessageDetail
              message={message}
              ongoing={chat?.window.ongoing && selectedIndex === count - 1}
              onBack={showList}
              viewActionsRef={viewActionsRef}
              roleLabel={roleLabel(message)}
              debugHint={false}
              emptyNotice="This Codex message records no text, tool call or result."
            />
          ) : (
            <div className="codex-empty">Loading Codex message...</div>
          )}
        </>
      );
    },
  };

  return {
    picker: {
      count: ctx.enabled ? conversations.length : 0,
      open: (index) => {
        const session = conversations[index];
        if (session) void openChat(session);
      },
      render: ({ selectedIndex: index, onHover }) => (
        <CodexPickerSection
          sessions={ctx.enabled ? conversations : []}
          loading={loading}
          error={error}
          searchQuery={ctx.searchQuery}
          selectedIndex={index}
          onHover={onHover}
          onOpen={(session) => void openChat(session)}
          onRefresh={() => void loadSessions()}
        />
      ),
    },
    hasOpenSession: ctx.enabled && chat !== null,
    backToSession: () => {
      if (chat && enabledRef.current) showList();
    },
    views: { list: listView, detail: detailView },
    onSettingsLoaded: async (settings) => {
      // Bootstrap calls the previous render's runtime immediately after updating settings.
      if (
        enabledRef.current ||
        settings?.plugins?.some((plugin) => plugin.id === "codex" && plugin.enabled)
      )
        await loadSessions();
    },
    onSettingsSaved: () => {
      if (enabledRef.current) void loadSessions();
    },
  };
}
