import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ViewState, SessionInfo } from "./types";
import { useSession } from "./hooks/useSession";
import { usePicker } from "./hooks/usePicker";
import { SessionPicker } from "./components/SessionPicker";
import { MessageList } from "./components/MessageList";
import { MessageDetail } from "./components/MessageDetail";
import { TeamBoard } from "./components/TeamBoard";
import { DebugViewer } from "./components/DebugViewer";
import { InfoBar } from "./components/InfoBar";
import { KeybindBar } from "./components/KeybindBar";
import { ViewToolbar } from "./components/ViewToolbar";

export function App() {
  const [view, setView] = useState<ViewState>("picker");
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [expandedMessages, setExpandedMessages] = useState<Set<number>>(
    new Set(),
  );
  const [pickerSelectedIndex, setPickerSelectedIndex] = useState(0);
  const [showKeybinds, setShowKeybinds] = useState(true);
  const [animFrame, setAnimFrame] = useState(0);

  const session = useSession();
  const picker = usePicker();

  // Animation frame for ongoing indicators
  useEffect(() => {
    if (!session.ongoing) return;
    const id = setInterval(() => {
      setAnimFrame((f) => f + 1);
    }, 100);
    return () => clearInterval(id);
  }, [session.ongoing]);

  // Auto-discover sessions on mount
  const discoveredRef = useRef(false);
  useEffect(() => {
    if (discoveredRef.current) return;
    discoveredRef.current = true;
    // Discover all project directories from ~/.claude/projects/
    const discover = async () => {
      try {
        const dirs = await invoke<string[]>("get_project_dirs");
        if (dirs.length > 0) {
          picker.discoverSessions(dirs);
        }
      } catch (err) {
        console.error("Failed to get project dirs:", err);
      }
    };
    discover();
  }, [picker.discoverSessions]);

  // Handle session selection from picker
  const handleSelectSession = useCallback(
    (sessionInfo: SessionInfo) => {
      session.loadSession(sessionInfo.path);
      setView("list");
      setSelectedMessage(0);
      setExpandedMessages(new Set());
    },
    [session.loadSession],
  );

  // Auto-select newest message (last index) when messages load
  useEffect(() => {
    if (session.messages.length > 0 && view === "list") {
      setSelectedMessage((prev) =>
        prev >= session.messages.length ? session.messages.length - 1 : prev
      );
    }
  }, [session.messages.length, view]);

  // Toggle message expand
  const toggleMessage = useCallback((index: number) => {
    setExpandedMessages((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  // Open detail view
  const openDetail = useCallback((index: number) => {
    setSelectedMessage(index);
    setView("detail");
  }, []);

  // -- Extracted action callbacks for toolbar + keyboard --

  const expandAll = useCallback(() => {
    setExpandedMessages((prev) => {
      const next = new Set(prev);
      session.messages.forEach((msg, i) => {
        if (msg.role === "claude") next.add(i);
      });
      return next;
    });
  }, [session.messages]);

  const collapseAll = useCallback(() => {
    setExpandedMessages(new Set());
  }, []);

  // Visual top = newest message = last index (display is reversed)
  const jumpToTop = useCallback(() => {
    setSelectedMessage(Math.max(session.messages.length - 1, 0));
  }, [session.messages.length]);

  // Visual bottom = oldest message = index 0
  const jumpToBottom = useCallback(() => {
    setSelectedMessage(0);
  }, []);

  const openDebug = useCallback(() => {
    if (session.sessionPath) {
      session.loadDebugLog(session.sessionPath);
      setView("debug");
    }
  }, [session.sessionPath, session.loadDebugLog]);

  const openTeams = useCallback(() => {
    if (session.teams.length > 0) setView("team");
  }, [session.teams.length]);

  const goToSessions = useCallback(() => {
    setView("picker");
  }, []);

  const backToList = useCallback(() => {
    if (session.sessionPath) setView("list");
  }, [session.sessionPath]);

  const toggleKeybinds = useCallback(() => {
    setShowKeybinds((v) => !v);
  }, []);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if an input is focused
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      switch (view) {
        case "list":
          handleListKeys(e);
          break;
        case "detail":
          handleDetailKeys(e);
          break;
        case "picker":
          handlePickerKeys(e);
          break;
        case "team":
          handleTeamKeys(e);
          break;
        case "debug":
          handleDebugKeys(e);
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  // -- Key handlers for each view --

  function handleListKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "j":
        e.preventDefault();
        // Display is reversed (newest first), so j (down) decreases index
        setSelectedMessage((i) => Math.max(i - 1, 0));
        break;
      case "k":
        e.preventDefault();
        setSelectedMessage((i) => Math.min(i + 1, session.messages.length - 1));
        break;
      case "G":
        e.preventDefault();
        // G = visual bottom = oldest message = index 0
        jumpToTop();
        break;
      case "g":
        e.preventDefault();
        // g = visual top = newest message = last index
        jumpToBottom();
        break;
      case "Tab":
        e.preventDefault();
        toggleMessage(selectedMessage);
        break;
      case "Enter":
        e.preventDefault();
        if (session.messages.length > 0) {
          openDetail(selectedMessage);
        }
        break;
      case "e":
        e.preventDefault();
        expandAll();
        break;
      case "c":
        e.preventDefault();
        collapseAll();
        break;
      case "t":
        e.preventDefault();
        openTeams();
        break;
      case "d":
        e.preventDefault();
        openDebug();
        break;
      case "q":
      case "Escape":
        e.preventDefault();
        goToSessions();
        break;
      case "s":
        e.preventDefault();
        goToSessions();
        break;
      case "?":
        e.preventDefault();
        toggleKeybinds();
        break;
    }
  }

  function handleDetailKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "q":
      case "Escape":
        e.preventDefault();
        setView("list");
        break;
      case "?":
        e.preventDefault();
        toggleKeybinds();
        break;
    }
  }

  function handlePickerKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "j":
        e.preventDefault();
        setPickerSelectedIndex((i) =>
          Math.min(i + 1, picker.sessions.length - 1),
        );
        break;
      case "k":
        e.preventDefault();
        setPickerSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        if (picker.sessions[pickerSelectedIndex]) {
          handleSelectSession(picker.sessions[pickerSelectedIndex]);
        }
        break;
      case "q":
      case "Escape":
        e.preventDefault();
        backToList();
        break;
      case "?":
        e.preventDefault();
        toggleKeybinds();
        break;
    }
  }

  function handleTeamKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "q":
      case "Escape":
        e.preventDefault();
        setView("list");
        break;
      case "?":
        e.preventDefault();
        toggleKeybinds();
        break;
    }
  }

  function handleDebugKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "q":
      case "Escape":
        e.preventDefault();
        setView("list");
        break;
      case "?":
        e.preventDefault();
        toggleKeybinds();
        break;
    }
  }

  // Keybind bar click actions
  const keybindActions: Record<string, () => void> = {};
  if (view === "list") {
    keybindActions["debug"] = openDebug;
    keybindActions["sessions"] = goToSessions;
    if (session.teams.length > 0) {
      keybindActions["tasks"] = openTeams;
    }
  } else if (view === "picker") {
    keybindActions["back"] = backToList;
  } else if (view === "detail") {
    keybindActions["back"] = () => setView("list");
  } else if (view === "team") {
    keybindActions["back"] = () => setView("list");
  } else if (view === "debug") {
    keybindActions["back"] = () => setView("list");
  }

  // Render the active view
  const renderView = () => {
    switch (view) {
      case "picker":
        return (
          <SessionPicker
            sessions={picker.sessions}
            loading={picker.loading}
            searchQuery={picker.searchQuery}
            selectedIndex={pickerSelectedIndex}
            onSelect={handleSelectSession}
            onSearchChange={picker.setSearchQuery}
            onSelectIndex={setPickerSelectedIndex}
            onBack={session.sessionPath ? backToList : undefined}
            animFrame={animFrame}
          />
        );

      case "list":
        return (
          <MessageList
            messages={session.messages}
            selectedIndex={selectedMessage}
            expandedSet={expandedMessages}
            ongoing={session.ongoing}
            animFrame={animFrame}
            onSelect={setSelectedMessage}
            onToggle={toggleMessage}
            onOpenDetail={openDetail}
          />
        );

      case "detail":
        if (
          session.messages.length > 0 &&
          selectedMessage < session.messages.length
        ) {
          return (
            <MessageDetail
              message={session.messages[selectedMessage]}
              onBack={() => setView("list")}
            />
          );
        }
        return null;

      case "team":
        return (
          <TeamBoard teams={session.teams} onBack={() => setView("list")} />
        );

      case "debug":
        return (
          <DebugViewer
            entries={session.debugEntries}
            onBack={() => setView("list")}
          />
        );
    }
  };

  return (
    <div className="app">
      {/* Info bar — only show when we have a loaded session */}
      {session.sessionPath && view !== "picker" && (
        <InfoBar
          meta={session.meta}
          gitInfo={session.gitInfo}
          messages={session.messages}
          ongoing={session.ongoing}
          animFrame={animFrame}
        />
      )}

      {/* View toolbar */}
      <ViewToolbar
        view={view}
        hasTeams={session.teams.length > 0}
        hasSession={!!session.sessionPath}
        messageCount={session.messages.length}
        onGoToSessions={goToSessions}
        onExpandAll={expandAll}
        onCollapseAll={collapseAll}
        onJumpTop={jumpToTop}
        onJumpBottom={jumpToBottom}
        onOpenTeams={openTeams}
        onOpenDebug={openDebug}
        onBackToList={backToList}
      />

      <div className="app-body">
        <div className="main-content">{renderView()}</div>
      </div>

      {/* Keybind bar */}
      <KeybindBar
        view={view}
        hasTeams={session.teams.length > 0}
        showHints={showKeybinds}
        onToggle={toggleKeybinds}
        actions={keybindActions}
      />
    </div>
  );
}
