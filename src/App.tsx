import { lazy, Suspense, useState, useEffect, useLayoutEffect, useCallback, useRef } from "react";
import { invoke, ApiAuthError } from "./lib/invoke";
import { onApiTokenChange } from "./lib/apiToken";
import type {
  ViewState,
  SessionInfo,
  DisplayMessage,
  AnalyticsSettings,
  EfficiencyAnalysisJob,
  EfficiencyFinding,
  PreparedEfficiencyPayload,
  SessionEfficiencyAnalysis,
} from "./types";
import { useSession } from "./hooks/useSession";
import { usePicker } from "./hooks/usePicker";
import { useToggleSet } from "./hooks/useToggleSet";
import { useKeyboard } from "./hooks/useKeyboard";
import { useViewActionsRef, useViewActionCallbacks } from "./hooks/useViewActions";
import { useFontScale } from "./hooks/useFontScale";
import { useRecapPreview } from "./hooks/useRecapPreview";
import { useEfficiencyJobs } from "./hooks/useEfficiencyJobs";
import { useCompletedEfficiencyDashboard } from "./hooks/useCompletedEfficiencyDashboard";
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
import { BetaBadge } from "./components/BetaBadge";
import { EfficiencyPrivacyModal } from "./components/EfficiencyPrivacyModal";
import { JevKeyRequiredModal } from "./components/JevKeyRequiredModal";
import { OperationResultModal } from "./components/OperationResultModal";
import {
  shouldRecycle,
  saveRestoreState,
  takeRestoreState,
  reloadWebview,
} from "./lib/webviewRecycle";

const EfficiencyPanel = lazy(() =>
  import("./components/EfficiencyPanel").then((module) => ({
    default: module.EfficiencyPanel,
  })),
);

const EfficiencyDashboardModal = lazy(() =>
  import("./components/EfficiencyDashboardModal").then((module) => ({
    default: module.EfficiencyDashboardModal,
  })),
);

export function App() {
  const [view, setView] = useState<ViewState>("picker");
  const [storedSelectedMessage, setSelectedMessage] = useState(0);
  const [pickerSelectedIndex, setPickerSelectedIndex] = useState(0);
  const [showKeybinds, setShowKeybinds] = useState(true);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [sidebarWidth, setSidebarWidth] = useState(180);
  const [sidebarFocused, setSidebarFocused] = useState(false);
  const [sidebarHighlight, setSidebarHighlight] = useState(0); // index in project list (0 = "All")
  const [showSettings, setShowSettings] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<
    "general" | "appearance" | "api" | "analytics"
  >("general");
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set());
  const [fontScale, setFontScale] = useFontScale();
  const [recapPreview, setRecapPreview] = useRecapPreview();
  // Full (heavy-body) message for the detail view, fetched on demand since the
  // list only holds lightened messages.
  const [detailMessage, setDetailMessage] = useState<DisplayMessage | null>(null);
  const [detailError, setDetailError] = useState(false);
  const [preparedEfficiencyPayload, setPreparedEfficiencyPayload] =
    useState<PreparedEfficiencyPayload | null>(null);
  const [startingEfficiency, setStartingEfficiency] = useState(false);
  const [showJevKeyRequired, setShowJevKeyRequired] = useState(false);
  const [efficiencyError, setEfficiencyError] = useState("");
  const [efficiencyAnalysis, setEfficiencyAnalysis] = useState<SessionEfficiencyAnalysis | null>(
    null,
  );
  const [efficiencyDashboard, setEfficiencyDashboard] = useState<{
    session: SessionInfo;
    analysis: SessionEfficiencyAnalysis | null;
    error: string;
  } | null>(null);
  const detailReqRef = useRef(0);
  const efficiencyDashboardReqRef = useRef(0);
  const preparedAnalysisOpensDashboardRef = useRef(false);
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
  // A failed analysis otherwise showed up only as the picker's button changing
  // to "Retry analysis", with the reason left on a job record nobody renders.
  const reportEfficiencyFailure = useCallback((job: EfficiencyAnalysisJob) => {
    const name = job.sessionName || job.sessionId;
    setEfficiencyError(`Jev analysis failed for ${name}: ${job.error || job.message}`);
  }, []);
  const efficiencyJobs = useEfficiencyJobs(reportEfficiencyFailure);
  // The open session's full SessionInfo (liveness, session_id), looked up from
  // the picker's list by path — useSession's meta only carries cwd/branch/mode.
  const selectedSessionInfo =
    picker.allSessions.find((s) => s.path === session.sessionPath) ?? null;
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

  // Whether this backend can focus a session's terminal window (macOS +
  // local, whether we're the Tauri app or a browser talking to the HTTP
  // API) — gates the Focus action instead of `isTauri`.
  const [canFocus, setCanFocus] = useState(false);

  // Set when the backend rejects this browser as an unaccepted client (HTTP
  // 401, see lib/apiToken.ts). Every API call would fail the same way — the
  // Settings modal included — so a banner replaces the usual bootstrap path.
  const [authError, setAuthError] = useState<string | null>(null);

  // Bootstrap: load settings, then either open Settings (no projects dir yet)
  // or discover sessions. A 401 means this browser is not an accepted client;
  // every other call would fail the same way, so stop at the banner.
  const discover = useCallback(async () => {
    let dirExists = false;
    try {
      const settings = await invoke<{
        projects_dir: string | null;
        effective_dir_exists: boolean;
        can_focus: boolean;
      }>("get_settings");
      dirExists = settings.effective_dir_exists;
      setCanFocus(settings.can_focus);
    } catch (err) {
      if (err instanceof ApiAuthError) {
        setAuthError(err.message);
        return;
      }
      // no settings file yet
    }
    if (!dirExists) {
      setShowSettings(true);
      return;
    }
    await loadProjectDirs();
  }, [loadProjectDirs]);

  // Auto-discover sessions on mount (once, even under StrictMode's double effect).
  const discoveredRef = useRef(false);
  const efficiencySummaryRevision = session.sessionPath
    ? efficiencyJobs.summariesBySession.get(session.sessionPath)?.analyzedAt
    : undefined;

  useEffect(() => {
    if (discoveredRef.current) return;
    discoveredRef.current = true;
    void discover();
  }, [discover]);

  // Recover from the banner when this tab's credential arrives or changes:
  // in dev/web mode the backend may start after the dev server (first run),
  // or `web-ui` may have been reissued from another client. The Vite plugin
  // pushes the new value over HMR, so re-run the bootstrap instead of leaving
  // a banner whose remedy (a restart) is no longer needed.
  //
  // Subscribed for the whole mount, not only while the banner is up. The
  // banner's state lands from a rejected promise, so React may flush the
  // passive effect that would subscribe a tick after the banner paints; a
  // credential arriving in that gap found no listener and the banner stuck.
  // The ref is written in a layout effect so the listener sees the banner
  // state from the same commit that painted it, not a passive tick later.
  const authErrorRef = useRef<string | null>(null);
  useLayoutEffect(() => {
    authErrorRef.current = authError;
  }, [authError]);
  useEffect(
    () =>
      onApiTokenChange((credential) => {
        if (!credential || !authErrorRef.current) return;
        setAuthError(null);
        void discover();
      }),
    [discover],
  );

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

  const requestEfficiencyAnalysis = useCallback(async (path: string, openDashboard = false) => {
    setEfficiencyError("");
    try {
      const settings = await invoke<AnalyticsSettings>("get_analytics_settings");
      if (!settings.jev.configured) {
        preparedAnalysisOpensDashboardRef.current = false;
        setShowJevKeyRequired(true);
        return;
      }
      const payload = await invoke<PreparedEfficiencyPayload>(
        "prepare_session_efficiency_payload",
        { path, payloadMode: settings.defaultPayloadMode },
      );
      preparedAnalysisOpensDashboardRef.current = openDashboard;
      setPreparedEfficiencyPayload(payload);
    } catch (error) {
      preparedAnalysisOpensDashboardRef.current = false;
      setEfficiencyError(String(error));
    }
  }, []);

  const closeEfficiencyDashboard = useCallback(() => {
    efficiencyDashboardReqRef.current += 1;
    setEfficiencyDashboard(null);
  }, []);

  const openEfficiencyDashboard = useCallback(async (sessionInfo: SessionInfo) => {
    const requestId = ++efficiencyDashboardReqRef.current;
    setEfficiencyDashboard({ session: sessionInfo, analysis: null, error: "" });
    try {
      const analysis = await invoke<SessionEfficiencyAnalysis | null>("get_session_efficiency", {
        path: sessionInfo.path,
      });
      if (requestId !== efficiencyDashboardReqRef.current) return;
      setEfficiencyDashboard({
        session: sessionInfo,
        analysis,
        error: analysis ? "" : "No completed efficiency analysis was found for this session.",
      });
    } catch (error) {
      if (requestId !== efficiencyDashboardReqRef.current) return;
      setEfficiencyDashboard({ session: sessionInfo, analysis: null, error: String(error) });
    }
  }, []);

  const openCompletedPickerDashboard = useCallback(
    (completedSessionPath: string) => {
      const sessionInfo = picker.allSessions.find(
        (candidate) => candidate.path === completedSessionPath,
      );
      if (sessionInfo) void openEfficiencyDashboard(sessionInfo);
    },
    [openEfficiencyDashboard, picker.allSessions],
  );
  const openDashboardWhenAnalysisCompletes = useCompletedEfficiencyDashboard(
    efficiencyJobs.jobs,
    openCompletedPickerDashboard,
  );

  const confirmEfficiencyAnalysis = useCallback(async () => {
    if (!preparedEfficiencyPayload) return;
    setStartingEfficiency(true);
    setEfficiencyError("");
    try {
      const job = await invoke<EfficiencyAnalysisJob>("start_session_efficiency_analysis", {
        path: preparedEfficiencyPayload.sessionPath,
        payload: preparedEfficiencyPayload,
      });
      if (preparedAnalysisOpensDashboardRef.current) {
        openDashboardWhenAnalysisCompletes(job);
      }
      preparedAnalysisOpensDashboardRef.current = false;
      setPreparedEfficiencyPayload(null);
      await efficiencyJobs.refresh();
    } catch (error) {
      preparedAnalysisOpensDashboardRef.current = false;
      setEfficiencyError(String(error));
    } finally {
      setStartingEfficiency(false);
    }
  }, [preparedEfficiencyPayload, efficiencyJobs, openDashboardWhenAnalysisCompletes]);

  useEffect(() => {
    if (!session.sessionPath) {
      // oxlint-disable-next-line react/set-state-in-effect -- synchronizing cached backend analysis with selected session
      setEfficiencyAnalysis(null);
      return;
    }
    let disposed = false;
    void invoke<SessionEfficiencyAnalysis | null>("get_session_efficiency", {
      path: session.sessionPath,
    })
      .then((analysis) => {
        if (
          !disposed &&
          (efficiencySummaryRevision === undefined ||
            analysis === null ||
            analysis.analyzedAt === efficiencySummaryRevision)
        ) {
          setEfficiencyAnalysis(analysis);
        }
      })
      .catch((error) => {
        if (!disposed) setEfficiencyError(String(error));
      });
    return () => {
      disposed = true;
    };
  }, [session.sessionPath, efficiencySummaryRevision]);

  const jumpToEfficiencyFinding = useCallback((finding: EfficiencyFinding) => {
    setSelectedMessage(finding.startMessageIndex);
    setView("list");
  }, []);

  // Restore whichever session was open right before a memory-driven webview
  // reload (see lib/webviewRecycle.ts), so the reload isn't disruptive.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    const pending = takeRestoreState();
    // This is the "synchronizing with an external system" case the rule carves out:
    // takeRestoreState() reads and *consumes* pending state from session storage, so it
    // cannot run during render, and the resulting navigation is inherently a state update.
    // oxlint-disable-next-line react/set-state-in-effect
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

  // Clamp the selection to the loaded message range during render rather than correcting it
  // from an effect. When a shorter session loads, the stored index can point past the end;
  // deriving here keeps every read in this render pass consistent, where the effect version
  // rendered once with the stale index before fixing it up.
  const selectedMessage =
    session.count > 0 && view === "list"
      ? Math.min(storedSelectedMessage, session.count - 1)
      : storedSelectedMessage;

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
          <div className="picker-dashboard">
            <SessionPicker
              sessions={picker.sessions}
              index={picker.index}
              loading={picker.loading}
              searchQuery={picker.searchQuery}
              selectedIndex={pickerSelectedIndex}
              onSelect={handleSelectSession}
              onSearchChange={picker.setSearchQuery}
              onSelectIndex={setPickerSelectedIndex}
              onVisiblePathsChange={picker.refresh}
              recapPreview={recapPreview}
              viewActionsRef={viewActionsRef}
              efficiencyJobs={efficiencyJobs.jobsBySession}
              efficiencySummaries={efficiencyJobs.summariesBySession}
              onAnalyse={(path) => void requestEfficiencyAnalysis(path, true)}
              onOpenEfficiencyDashboard={(sessionInfo) => void openEfficiencyDashboard(sessionInfo)}
            />
          </div>
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
        // Root CSS zoom invalidates Virtuoso's cached row geometry. Remount at
        // the new scale so its first measurements all use one coordinate space.
        return (
          <div className="session-view">
            <div className="session-efficiency-action">
              {efficiencyAnalysis ? (
                <Suspense fallback={<span className="braille-spinner" />}>
                  <EfficiencyPanel
                    analysis={efficiencyAnalysis}
                    currentTurns={session.count}
                    onReanalyse={() =>
                      session.sessionPath &&
                      void requestEfficiencyAnalysis(session.sessionPath, true)
                    }
                    onJumpToFinding={jumpToEfficiencyFinding}
                  />
                </Suspense>
              ) : (
                <button
                  type="button"
                  className="settings-modal__btn"
                  onClick={() =>
                    session.sessionPath && void requestEfficiencyAnalysis(session.sessionPath, true)
                  }
                  disabled={
                    !session.sessionPath ||
                    !["completed", "failed", "cancelled", undefined].includes(
                      efficiencyJobs.jobsBySession.get(session.sessionPath)?.status,
                    )
                  }
                >
                  Analyse efficiency <BetaBadge />
                </button>
              )}
            </div>
            <MessageList
              key={fontScale}
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
              findings={efficiencyAnalysis?.findings}
            />
          </div>
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
      {authError && (
        <div className="app-auth-banner" role="alert">
          <strong>Not an accepted client.</strong> This browser did not present a valid{" "}
          <code>web-ui</code> client credential, so the backend refused the connection. In dev/web
          mode restart <code>cctrace --web</code>; in Docker open the UI via localhost or an allowed
          origin. If <code>web-ui</code> was revoked, reissue it from the desktop app or another
          client.
          <span className="app-auth-banner__detail">{authError}</span>
        </div>
      )}
      {/* Info bar — only show when we have a loaded session */}
      {session.sessionPath && view !== "picker" && (
        <InfoBar
          meta={session.meta}
          gitInfo={session.gitInfo}
          contextTokens={session.contextTokens}
          sessionTotals={session.sessionTotals}
          sessionPath={session.sessionPath}
          ongoing={session.ongoing}
          sessionInfo={selectedSessionInfo}
          canFocus={canFocus}
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
        onOpenSettings={() => {
          setSettingsInitialTab("general");
          setShowSettings(true);
        }}
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
        index={picker.index}
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
          initialTab={settingsInitialTab}
        />
      )}
      {preparedEfficiencyPayload && (
        <EfficiencyPrivacyModal
          payload={preparedEfficiencyPayload}
          busy={startingEfficiency}
          onCancel={() => {
            preparedAnalysisOpensDashboardRef.current = false;
            setPreparedEfficiencyPayload(null);
          }}
          onConfirm={() => void confirmEfficiencyAnalysis()}
        />
      )}
      {efficiencyDashboard && (
        <Suspense fallback={null}>
          <EfficiencyDashboardModal
            sessionName={
              efficiencyDashboard.session.name ||
              efficiencyDashboard.session.first_message ||
              efficiencyDashboard.session.session_id
            }
            currentTurns={efficiencyDashboard.session.turn_count}
            analysis={efficiencyDashboard.analysis}
            error={efficiencyDashboard.error}
            onClose={closeEfficiencyDashboard}
            onReanalyse={() => {
              const path = efficiencyDashboard.session.path;
              closeEfficiencyDashboard();
              void requestEfficiencyAnalysis(path, true);
            }}
            onJumpToFinding={(finding) => {
              const sessionInfo = efficiencyDashboard.session;
              closeEfficiencyDashboard();
              handleSelectSession(sessionInfo);
              jumpToEfficiencyFinding(finding);
            }}
          />
        </Suspense>
      )}
      {showJevKeyRequired && (
        <JevKeyRequiredModal
          onCancel={() => setShowJevKeyRequired(false)}
          onOpenSettings={() => {
            setShowJevKeyRequired(false);
            setSettingsInitialTab("analytics");
            setShowSettings(true);
          }}
        />
      )}
      {efficiencyError && (
        <OperationResultModal
          kind="error"
          message={efficiencyError}
          onClose={() => setEfficiencyError("")}
        />
      )}
    </div>
  );
}
