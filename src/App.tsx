import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "./lib/invoke";
import type { ViewState, SessionInfo, DisplayMessage } from "./types";
import { useSession } from "./hooks/useSession";
import { usePicker } from "./hooks/usePicker";
import { useToggleSet } from "./hooks/useToggleSet";
import { useKeyboard } from "./hooks/useKeyboard";
import { useViewActionsRef, useViewActionCallbacks } from "./hooks/useViewActions";
import { useFontScale } from "./hooks/useFontScale";
import { useRecapPreview } from "./hooks/useRecapPreview";
import { SessionPicker } from "./components/SessionPicker";
import { MessageList } from "./components/MessageList";
import { MessageDetail } from "./components/MessageDetail";
import { TeamBoard } from "./components/TeamBoard";
import { DebugViewer } from "./components/DebugViewer";
import { InfoBar } from "./components/InfoBar";
import { KeybindBar } from "./components/KeybindBar";
import { ViewToolbar } from "./components/ViewToolbar";
import { ProjectTree, useProjectKeys, useProjectItems } from "./components/ProjectTree";
import { ResizeHandle } from "./components/ResizeHandle";
import { SettingsModal } from "./components/SettingsModal";
import {
  shouldRecycle,
  saveRestoreState,
  takeRestoreState,
  reloadWebview,
} from "./lib/webviewRecycle";

export function App() {
  const [view, setView] = useState<ViewState>("picker");
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [pickerSelectedIndex, setPickerSelectedIndex] = useState(0);
  const [showKeybinds, setShowKeybinds] = useState(true);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(180);
  const [sidebarFocused, setSidebarFocused] = useState(false);
  const [sidebarHighlight, setSidebarHighlight] = useState(0); // index in project list (0 = "All")
  const [showSettings, setShowSettings] = useState(false);
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set());
  const [fontScale, setFontScale] = useFontScale();
  const [recapPreview, setRecapPreview] = useRecapPreview();
  // Full (heavy-body) message for the detail view, fetched on demand since the
  // list only holds lightened messages.
  const [detailMessage, setDetailMessage] = useState<DisplayMessage | null>(null);
  const [detailError, setDetailError] = useState(false);
  const detailReqRef = useRef(0);
  // Counts session opens this page lifetime, to periodically recycle the
  // webview (see lib/webviewRecycle.ts for why).
  const switchCountRef = useRef(0);

  const toggleCollapse = useCallback((key: string) => {
    setCollapsedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const handleSelectProject = useCallback(
    (project: string | null) => {
      setSelectedProject(project);
      setPickerSelectedIndex(0);
      setSidebarFocused(false);
      if (view !== "picker") setView("picker");
    },
    [view],
  );

  const session = useSession();
  const picker = usePicker(selectedProject);
  const projectKeys = useProjectKeys(picker.allSessions, collapsedKeys);
  const projectItems = useProjectItems(picker.allSessions, collapsedKeys);

  const { loadSession, loadDebugLog, sessionPath } = session;
  const { discoverSessions, updateSessionOngoing } = picker;

  const {
    set: expandedMessages,
    toggle: toggleMessage,
    clear: clearExpanded,
    addAll: expandMessages,
  } = useToggleSet();

  // Shared: fetch project dirs and discover sessions
  const loadProjectDirs = useCallback(async () => {
    try {
      const dirs = await invoke<string[]>("get_project_dirs");
      if (dirs.length > 0) {
        await discoverSessions(dirs);
      }
    } catch (err) {
      console.error("Failed to get project dirs:", err);
    }
  }, [discoverSessions]);

  // Auto-discover sessions on mount; show settings if no path configured
  const discoveredRef = useRef(false);
  useEffect(() => {
    if (discoveredRef.current) return;
    discoveredRef.current = true;
    const discover = async () => {
      let dirExists = false;
      try {
        const settings = await invoke<{
          projects_dir: string | null;
          effective_dir_exists: boolean;
        }>("get_settings");
        dirExists = settings.effective_dir_exists;
      } catch {
        // no settings file yet
      }
      if (!dirExists) {
        setShowSettings(true);
        return;
      }
      await loadProjectDirs();
    };
    void discover();
  }, [loadProjectDirs]);

  // Sync session watcher's ongoing status to picker (avoids race condition
  // where picker watcher emits before session watcher updates).
  useEffect(() => {
    if (session.sessionPath) {
      updateSessionOngoing(session.sessionPath, session.ongoing);
    }
  }, [session.sessionPath, session.ongoing, updateSessionOngoing]);

  const openSessionByPath = useCallback(
    (path: string) => {
      void loadSession(path);
      setView("list");
      setSelectedMessage(0);
      clearExpanded();
      // Release the previous session's full (heavy-body) detail message, if
      // any was fetched — otherwise it stays retained in state indefinitely.
      setDetailMessage(null);
      setDetailError(false);
    },
    [loadSession, clearExpanded],
  );

  // Restore whichever session was open right before a memory-driven webview
  // reload (see lib/webviewRecycle.ts), so the reload isn't disruptive.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    const pending = takeRestoreState();
    if (pending) openSessionByPath(pending.sessionPath);
  }, [openSessionByPath]);

  // Handle session selection from picker
  const handleSelectSession = useCallback(
    (sessionInfo: SessionInfo) => {
      switchCountRef.current += 1;
      if (shouldRecycle(switchCountRef.current)) {
        saveRestoreState({ sessionPath: sessionInfo.path });
        // Reload is async (waits for in-flight invokes to settle first — see
        // webviewRecycle.ts). If it ever throws, fall back to opening the
        // session normally rather than silently doing nothing.
        void reloadWebview().catch(() => openSessionByPath(sessionInfo.path));
        return;
      }
      openSessionByPath(sessionInfo.path);
    },
    [openSessionByPath],
  );

  // Auto-select newest message (last index) when messages load
  useEffect(() => {
    if (session.count > 0 && view === "list") {
      setSelectedMessage((prev) => (prev >= session.count ? session.count - 1 : prev));
    }
  }, [session.count, view]);

  // Open detail view. The list holds only lightened messages, so fetch the full
  // (heavy-body) message on demand. Guard against out-of-order resolves when the
  // user opens several messages quickly, and surface failures instead of hanging
  // on the loading spinner forever.
  const openDetail = useCallback(
    (index: number) => {
      const req = ++detailReqRef.current;
      setSelectedMessage(index);
      setDetailMessage(null);
      setDetailError(false);
      setView("detail");
      // Leaving the list view — drop its loaded pages so they don't sit in
      // memory while Detail is showing. Coming back re-fetches the visible
      // window fresh instead of holding onto stale data indefinitely.
      session.clearWindow();
      session
        .loadFullMessage(index)
        .then((msg) => {
          if (req !== detailReqRef.current) return;
          if (msg) setDetailMessage(msg);
          else setDetailError(true);
        })
        .catch(() => {
          if (req === detailReqRef.current) setDetailError(true);
        });
    },
    [session],
  );

  // -- View actions: each view registers its own expand/collapse handlers --

  const viewActionsRef = useViewActionsRef();
  const { expandAll, collapseAll, scrollToTop, scrollToBottom } =
    useViewActionCallbacks(viewActionsRef);

  // Register message list expand/collapse when in list view. Uses the role
  // index so it works over the whole session without loading every body.
  const listExpandAll = useCallback(() => {
    const claudeIndices: number[] = [];
    session.roles.forEach((role, i) => {
      if (role === "claude") claudeIndices.push(i);
    });
    expandMessages(claudeIndices);
  }, [session.roles, expandMessages]);

  // Visual top = newest message = last index (display is reversed)
  const jumpToTop = useCallback(() => {
    setSelectedMessage(Math.max(session.count - 1, 0));
  }, [session.count]);

  // Visual bottom = oldest message = index 0
  const jumpToBottom = useCallback(() => {
    setSelectedMessage(0);
  }, []);

  const openDebug = useCallback(() => {
    if (sessionPath) {
      void loadDebugLog(sessionPath);
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
    if (!sessionPath) return;
    setView("list");
    // Leaving Detail — release the full (heavy-body) message it held so it
    // doesn't linger in memory until the next Detail open overwrites it.
    setDetailMessage(null);
    setDetailError(false);
  }, [sessionPath]);

  const toggleKeybinds = useCallback(() => {
    setShowKeybinds((v) => !v);
  }, []);

  const selectProjectByIndex = useCallback(
    (index: number) => {
      if (index >= 0 && index < projectKeys.length) {
        handleSelectProject(projectKeys[index]);
      }
    },
    [projectKeys, handleSelectProject],
  );

  // Keyboard navigation — build keyMap per view
  const keyMap: Record<string, () => void> = {};

  // Sidebar-focused shortcuts (override main shortcuts when sidebar has focus)
  if (sidebarFocused) {
    const sidebarDown = () => setSidebarHighlight((i) => Math.min(i + 1, projectKeys.length - 1));
    const sidebarUp = () => setSidebarHighlight((i) => Math.max(i - 1, 0));
    keyMap["j"] = sidebarDown;
    keyMap["ArrowDown"] = sidebarDown;
    keyMap["k"] = sidebarUp;
    keyMap["ArrowUp"] = sidebarUp;
    keyMap["Enter"] = () => selectProjectByIndex(sidebarHighlight);
    keyMap["Escape"] = () => setSidebarFocused(false);
    keyMap["l"] = () => setSidebarFocused(false);
    keyMap[" "] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key) toggleCollapse(item.key);
    };
    keyMap["ArrowRight"] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key && !item.isExpanded) toggleCollapse(item.key);
      else setSidebarFocused(false);
    };
    keyMap["ArrowLeft"] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key && item.isExpanded) toggleCollapse(item.key);
    };
    keyMap["?"] = toggleKeybinds;
  } else {
    switch (view) {
      case "list": {
        const moveDown = () => setSelectedMessage((i) => Math.min(i + 1, session.count - 1));
        const moveUp = () => setSelectedMessage((i) => Math.max(i - 1, 0));
        keyMap["j"] = moveDown;
        keyMap["ArrowDown"] = moveDown;
        keyMap["k"] = moveUp;
        keyMap["ArrowUp"] = moveUp;
        keyMap["G"] = jumpToTop;
        keyMap["g"] = jumpToBottom;
        keyMap["Tab"] = () => toggleMessage(selectedMessage);
        keyMap["Enter"] = () => {
          if (session.count > 0) openDetail(selectedMessage);
        };
        keyMap["e"] = expandAll;
        keyMap["c"] = collapseAll;
        keyMap["t"] = openTeams;
        keyMap["d"] = openDebug;
        keyMap["q"] = goToSessions;
        keyMap["Escape"] = goToSessions;
        keyMap["s"] = goToSessions;
        keyMap["?"] = toggleKeybinds;
        keyMap["h"] = () => setSidebarFocused(true);
        keyMap["ArrowLeft"] = () => setSidebarFocused(true);
        break;
      }
      case "detail":
        // j/k/Tab/Enter/q/Escape handled by MessageDetail's own useKeyboard
        keyMap["?"] = toggleKeybinds;
        break;
      case "picker": {
        const pickerDown = () =>
          setPickerSelectedIndex((i) => Math.min(i + 1, picker.sessions.length - 1));
        const pickerUp = () => setPickerSelectedIndex((i) => Math.max(i - 1, 0));
        keyMap["j"] = pickerDown;
        keyMap["ArrowDown"] = pickerDown;
        keyMap["k"] = pickerUp;
        keyMap["ArrowUp"] = pickerUp;
        keyMap["Enter"] = () => {
          if (picker.sessions[pickerSelectedIndex])
            handleSelectSession(picker.sessions[pickerSelectedIndex]);
        };
        keyMap["q"] = backToList;
        keyMap["Escape"] = backToList;
        keyMap["?"] = toggleKeybinds;
        keyMap["h"] = () => setSidebarFocused(true);
        keyMap["ArrowLeft"] = () => setSidebarFocused(true);
        break;
      }
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
            onVisiblePathsChange={picker.refresh}
            recapPreview={recapPreview}
            viewActionsRef={viewActionsRef}
          />
        );

      case "list":
        if (session.loading) {
          return (
            <div className="session-loading">
              <span className="braille-spinner" />
              Loading session...
            </div>
          );
        }
        return (
          <MessageList
            count={session.count}
            getMessage={session.getMessage}
            roles={session.roles}
            selectedIndex={selectedMessage}
            expandedSet={expandedMessages}
            ongoing={session.ongoing}
            onRangeChange={session.ensureRange}
            onSelect={setSelectedMessage}
            onToggle={toggleMessage}
            onOpenDetail={openDetail}
            viewActionsRef={viewActionsRef}
            onExpandAll={listExpandAll}
            onCollapseAll={clearExpanded}
          />
        );

      case "detail": {
        if (detailMessage) {
          return (
            <MessageDetail
              message={detailMessage}
              ongoing={session.ongoing}
              onBack={backToList}
              viewActionsRef={viewActionsRef}
            />
          );
        }
        if (detailError) {
          return (
            <div className="session-loading">
              Failed to load message.{" "}
              <button className="link-button" onClick={backToList}>
                Back
              </button>
            </div>
          );
        }
        // Full body still loading (fetched on demand from the cached build).
        if (selectedMessage < session.count) {
          return (
            <div className="session-loading">
              <span className="braille-spinner" />
              Loading message...
            </div>
          );
        }
        return null;
      }

      case "team":
        return <TeamBoard teams={session.teams} />;

      case "debug":
        return <DebugViewer entries={session.debugEntries} viewActionsRef={viewActionsRef} />;
    }
  };

  return (
    <div className="app">
      {/* Info bar — only show when we have a loaded session */}
      {session.sessionPath && view !== "picker" && (
        <InfoBar
          meta={session.meta}
          gitInfo={session.gitInfo}
          contextTokens={session.contextTokens}
          sessionTotals={session.sessionTotals}
          sessionPath={session.sessionPath}
          ongoing={session.ongoing}
        />
      )}

      {/* View toolbar */}
      <ViewToolbar
        view={view}
        hasTeams={session.teams.length > 0}
        hasSession={!!session.sessionPath}
        onGoToSessions={goToSessions}
        onExpandAll={expandAll}
        onCollapseAll={collapseAll}
        onScrollToTop={scrollToTop}
        onScrollToBottom={scrollToBottom}
        onOpenTeams={openTeams}
        onOpenDebug={openDebug}
        onBackToList={backToList}
        onOpenSettings={() => setShowSettings(true)}
      />

      <div className="app-body">
        <ProjectTree
          sessions={picker.allSessions}
          selectedProject={selectedProject}
          highlightedIndex={sidebarHighlight}
          isFocused={sidebarFocused}
          collapsedKeys={collapsedKeys}
          onSelectProject={handleSelectProject}
          onToggleCollapse={toggleCollapse}
          onRefresh={loadProjectDirs}
          onFocus={() => setSidebarFocused(true)}
          refreshing={picker.loading}
          style={{ width: sidebarWidth, minWidth: 100, maxWidth: 400 }}
        />
        <ResizeHandle onResize={setSidebarWidth} />
        <div className="main-content" onClick={() => setSidebarFocused(false)}>
          {renderView()}
        </div>
      </div>

      {/* Keybind bar */}
      <KeybindBar
        view={view}
        hasTeams={session.teams.length > 0}
        showHints={showKeybinds}
        onToggle={toggleKeybinds}
        actions={keybindActions}
      />

      {showSettings && (
        <SettingsModal
          onClose={() => setShowSettings(false)}
          onSaved={loadProjectDirs}
          fontScale={fontScale}
          onFontScaleChange={setFontScale}
          recapPreview={recapPreview}
          onRecapPreviewChange={setRecapPreview}
        />
      )}
    </div>
  );
}
