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

export function App() {
  const [view, setView] = useState<ViewState>("picker");
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [expandedMessages, setExpandedMessages] = useState<Set<number>>(
    new Set()
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
    [session.loadSession]
  );

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
        setSelectedMessage((i) =>
          Math.min(i + 1, session.messages.length - 1)
        );
        break;
      case "k":
        e.preventDefault();
        setSelectedMessage((i) => Math.max(i - 1, 0));
        break;
      case "G":
        e.preventDefault();
        setSelectedMessage(Math.max(session.messages.length - 1, 0));
        break;
      case "g":
        e.preventDefault();
        setSelectedMessage(0);
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
        setExpandedMessages((prev) => {
          const next = new Set(prev);
          session.messages.forEach((msg, i) => {
            if (msg.role === "claude") next.add(i);
          });
          return next;
        });
        break;
      case "c":
        e.preventDefault();
        setExpandedMessages(new Set());
        break;
      case "t":
        e.preventDefault();
        if (session.teams.length > 0) setView("team");
        break;
      case "d":
        e.preventDefault();
        if (session.sessionPath) {
          session.loadDebugLog(session.sessionPath);
          setView("debug");
        }
        break;
      case "q":
      case "Escape":
        e.preventDefault();
        setView("picker");
        break;
      case "s":
        e.preventDefault();
        setView("picker");
        break;
      case "?":
        e.preventDefault();
        setShowKeybinds((v) => !v);
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
        setShowKeybinds((v) => !v);
        break;
    }
  }

  function handlePickerKeys(e: KeyboardEvent) {
    switch (e.key) {
      case "j":
        e.preventDefault();
        setPickerSelectedIndex((i) =>
          Math.min(i + 1, picker.sessions.length - 1)
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
        if (session.sessionPath) setView("list");
        break;
      case "?":
        e.preventDefault();
        setShowKeybinds((v) => !v);
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
        setShowKeybinds((v) => !v);
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
        setShowKeybinds((v) => !v);
        break;
    }
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

      <div className="app-body">
        <div className="main-content">{renderView()}</div>
      </div>

      {/* Keybind bar */}
      {showKeybinds && (
        <KeybindBar view={view} hasTeams={session.teams.length > 0} />
      )}
    </div>
  );
}
