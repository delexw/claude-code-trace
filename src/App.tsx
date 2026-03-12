import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ViewState, SessionInfo } from "./types";
import { useSession } from "./hooks/useSession";
import { usePicker } from "./hooks/usePicker";
import { useToggleSet } from "./hooks/useToggleSet";
import { useKeyboard } from "./hooks/useKeyboard";
import { SessionPicker } from "./components/SessionPicker";
import { MessageList } from "./components/MessageList";
import { MessageDetail } from "./components/MessageDetail";
import { TeamBoard } from "./components/TeamBoard";
import { DebugViewer } from "./components/DebugViewer";
import { InfoBar } from "./components/InfoBar";
import { KeybindBar } from "./components/KeybindBar";
import { ViewToolbar } from "./components/ViewToolbar";
import { ProjectTree } from "./components/ProjectTree";

export function App() {
  const [view, setView] = useState<ViewState>("picker");
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [pickerSelectedIndex, setPickerSelectedIndex] = useState(0);
  const [showKeybinds, setShowKeybinds] = useState(true);
  const [animFrame, setAnimFrame] = useState(0);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);

  const handleSelectProject = useCallback(
    (project: string | null) => {
      setSelectedProject(project);
      setPickerSelectedIndex(0);
      if (view !== "picker") setView("picker");
    },
    [view],
  );

  const session = useSession();
  const picker = usePicker(selectedProject);

  const { loadSession, loadDebugLog, sessionPath } = session;
  const { discoverSessions } = picker;

  const {
    set: expandedMessages,
    toggle: toggleMessage,
    clear: clearExpanded,
    addAll: expandMessages,
  } = useToggleSet();

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
    const discover = async () => {
      try {
        const dirs = await invoke<string[]>("get_project_dirs");
        if (dirs.length > 0) {
          discoverSessions(dirs);
        }
      } catch (err) {
        console.error("Failed to get project dirs:", err);
      }
    };
    discover();
  }, [discoverSessions]);

  // Handle session selection from picker
  const handleSelectSession = useCallback(
    (sessionInfo: SessionInfo) => {
      loadSession(sessionInfo.path);
      setView("list");
      setSelectedMessage(0);
      clearExpanded();
    },
    [loadSession, clearExpanded],
  );

  // Auto-select newest message (last index) when messages load
  useEffect(() => {
    if (session.messages.length > 0 && view === "list") {
      setSelectedMessage((prev) =>
        prev >= session.messages.length ? session.messages.length - 1 : prev,
      );
    }
  }, [session.messages.length, view]);

  // Open detail view
  const openDetail = useCallback((index: number) => {
    setSelectedMessage(index);
    setView("detail");
  }, []);

  // -- Extracted action callbacks for toolbar + keyboard --

  const expandAll = useCallback(() => {
    const claudeIndices: number[] = [];
    session.messages.forEach((msg, i) => {
      if (msg.role === "claude") claudeIndices.push(i);
    });
    expandMessages(claudeIndices);
  }, [session.messages, expandMessages]);

  const collapseAll = useCallback(() => {
    clearExpanded();
  }, [clearExpanded]);

  // Visual top = newest message = last index (display is reversed)
  const jumpToTop = useCallback(() => {
    setSelectedMessage(Math.max(session.messages.length - 1, 0));
  }, [session.messages.length]);

  // Visual bottom = oldest message = index 0
  const jumpToBottom = useCallback(() => {
    setSelectedMessage(0);
  }, []);

  const openDebug = useCallback(() => {
    if (sessionPath) {
      loadDebugLog(sessionPath);
      setView("debug");
    }
  }, [sessionPath, loadDebugLog]);

  const openTeams = useCallback(() => {
    if (session.teams.length > 0) setView("team");
  }, [session.teams.length]);

  const goToSessions = useCallback(() => {
    setView("picker");
  }, []);

  const backToList = useCallback(() => {
    if (sessionPath) setView("list");
  }, [sessionPath]);

  const toggleKeybinds = useCallback(() => {
    setShowKeybinds((v) => !v);
  }, []);

  // Keyboard navigation — build keyMap per view
  const keyMap: Record<string, () => void> = {};
  switch (view) {
    case "list":
      keyMap["j"] = () => setSelectedMessage((i) => Math.max(i - 1, 0));
      keyMap["k"] = () => setSelectedMessage((i) => Math.min(i + 1, session.messages.length - 1));
      keyMap["G"] = jumpToTop;
      keyMap["g"] = jumpToBottom;
      keyMap["Tab"] = () => toggleMessage(selectedMessage);
      keyMap["Enter"] = () => {
        if (session.messages.length > 0) openDetail(selectedMessage);
      };
      keyMap["e"] = expandAll;
      keyMap["c"] = collapseAll;
      keyMap["t"] = openTeams;
      keyMap["d"] = openDebug;
      keyMap["q"] = goToSessions;
      keyMap["Escape"] = goToSessions;
      keyMap["s"] = goToSessions;
      keyMap["?"] = toggleKeybinds;
      break;
    case "detail":
      keyMap["q"] = () => setView("list");
      keyMap["Escape"] = () => setView("list");
      keyMap["?"] = toggleKeybinds;
      break;
    case "picker":
      keyMap["j"] = () =>
        setPickerSelectedIndex((i) => Math.min(i + 1, picker.sessions.length - 1));
      keyMap["k"] = () => setPickerSelectedIndex((i) => Math.max(i - 1, 0));
      keyMap["Enter"] = () => {
        if (picker.sessions[pickerSelectedIndex])
          handleSelectSession(picker.sessions[pickerSelectedIndex]);
      };
      keyMap["q"] = backToList;
      keyMap["Escape"] = backToList;
      keyMap["?"] = toggleKeybinds;
      break;
    case "team":
      keyMap["q"] = () => setView("list");
      keyMap["Escape"] = () => setView("list");
      keyMap["?"] = toggleKeybinds;
      break;
    case "debug":
      keyMap["q"] = () => setView("list");
      keyMap["Escape"] = () => setView("list");
      keyMap["?"] = toggleKeybinds;
      break;
  }
  useKeyboard(keyMap);

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
        if (session.messages.length > 0 && selectedMessage < session.messages.length) {
          return (
            <MessageDetail
              message={session.messages[selectedMessage]}
              onBack={() => setView("list")}
            />
          );
        }
        return null;

      case "team":
        return <TeamBoard teams={session.teams} onBack={() => setView("list")} />;

      case "debug":
        return <DebugViewer entries={session.debugEntries} onBack={() => setView("list")} />;
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
          sessionTotals={session.sessionTotals}
          sessionPath={session.sessionPath}
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
        <ProjectTree
          sessions={picker.allSessions}
          selectedProject={selectedProject}
          onSelectProject={handleSelectProject}
        />
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
