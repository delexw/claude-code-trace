import {
  lazy,
  Suspense,
  useState,
  useEffect,
  useCallback,
  useRef,
  type CSSProperties,
} from "react";
import { invoke } from "../../lib/invoke";
import type {
  SessionInfo,
  DisplayMessage,
  AnalyticsSettings,
  EfficiencyAnalysisJob,
  EfficiencyFinding,
  PreparedEfficiencyPayload,
  SessionEfficiencyAnalysis,
  SettingsResponse,
} from "../../types";
import type { KeyMap, SourceContext, SourceRuntime, SourceView } from "../types";
import { useSession } from "../../hooks/useSession";
import { usePicker } from "../../hooks/usePicker";
import { useToggleSet } from "../../hooks/useToggleSet";
import { useEfficiencyJobs } from "../../hooks/useEfficiencyJobs";
import { useCompletedEfficiencyDashboard } from "../../hooks/useCompletedEfficiencyDashboard";
import { MessageList } from "../../components/MessageList";
import { MessageDetail } from "../../components/MessageDetail";
import { TeamBoard } from "../../components/TeamBoard";
import { DebugViewer } from "../../components/DebugViewer";
import { InfoBar } from "../../components/InfoBar";
import { IndexProgressBar } from "../../components/IndexProgressBar";
import { ProjectTree, useProjectKeys, useProjectItems } from "../../components/ProjectTree";
import { BetaBadge } from "../../components/BetaBadge";
import { EfficiencyPrivacyModal } from "../../components/EfficiencyPrivacyModal";
import { JevKeyRequiredModal } from "../../components/JevKeyRequiredModal";
import { OperationResultModal } from "../../components/OperationResultModal";
import {
  shouldRecycle,
  saveRestoreState,
  takeRestoreState,
  reloadWebview,
} from "../../lib/webviewRecycle";
import { ClaudeHeaderStats, ClaudeSessionList } from "./ClaudeSessionList";
import { claudeKeyHints, type ClaudeView } from "./keyHints";
import { ClaudeToolbar } from "./ClaudeToolbar";

const EfficiencyPanel = lazy(() =>
  import("../../components/EfficiencyPanel").then((module) => ({
    default: module.EfficiencyPanel,
  })),
);

const EfficiencyDashboardModal = lazy(() =>
  import("../../components/EfficiencyDashboardModal").then((module) => ({
    default: module.EfficiencyDashboardModal,
  })),
);

export function useClaudeSource(ctx: SourceContext): SourceRuntime {
  const { openView, goToPicker, setPickerIndex, openSettings } = ctx;
  const view = ctx.activeView as ClaudeView | null;

  const [storedSelectedMessage, setSelectedMessage] = useState(0);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [sidebarFocused, setSidebarFocused] = useState(false);
  const [sidebarHighlight, setSidebarHighlight] = useState(0); // index in project list (0 = "All")
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set());
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
      setPickerIndex(0);
      setSidebarFocused(false);
      goToPicker();
    },
    [setPickerIndex, goToPicker],
  );

  const session = useSession();
  const picker = usePicker(selectedProject, ctx.searchQuery);
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
  const canFocus = ctx.settings?.can_focus ?? false;

  // Bootstrap: open Settings when there is no projects dir yet, otherwise
  // discover sessions.
  const onSettingsLoaded = useCallback(
    async (settings: SettingsResponse | null) => {
      if (!settings?.effective_dir_exists) {
        openSettings();
        return;
      }
      await loadProjectDirs();
    },
    [openSettings, loadProjectDirs],
  );

  const efficiencySummaryRevision = session.sessionPath
    ? efficiencyJobs.summariesBySession.get(session.sessionPath)?.analyzedAt
    : undefined;

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
      openView("list");
      setSelectedMessage(0);
      clearExpanded();
      // Release the previous session's full (heavy-body) detail message, if
      // any was fetched — otherwise it stays retained in state indefinitely.
      setDetailMessage(null);
      setDetailError(false);
    },
    [loadSession, openView, clearExpanded],
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

  const jumpToEfficiencyFinding = useCallback(
    (finding: EfficiencyFinding) => {
      setSelectedMessage(finding.startMessageIndex);
      openView("list");
    },
    [openView],
  );

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
      openView("detail");
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
    [session, openView],
  );

  const { viewActionsRef } = ctx;

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
      openView("debug");
    }
  }, [sessionPath, loadDebugLog, openView]);

  const openTeams = useCallback(() => {
    if (session.teams.length > 0) openView("team");
  }, [session.teams.length, openView]);

  const backToList = useCallback(() => {
    if (!sessionPath) return;
    openView("list");
    // Leaving Detail — release the full (heavy-body) message it held so it
    // doesn't linger in memory until the next Detail open overwrites it.
    setDetailMessage(null);
    setDetailError(false);
  }, [sessionPath, openView]);

  const showList = useCallback(() => openView("list"), [openView]);

  const selectProjectByIndex = useCallback(
    (index: number) => {
      if (index >= 0 && index < projectKeys.length) {
        handleSelectProject(projectKeys[index]);
      }
    },
    [projectKeys, handleSelectProject],
  );

  const focusSidebar = useCallback(() => setSidebarFocused(true), []);
  const blurSidebar = useCallback(() => setSidebarFocused(false), []);

  const sidebarKeyMap: KeyMap = {};
  if (sidebarFocused) {
    const sidebarDown = () => setSidebarHighlight((i) => Math.min(i + 1, projectKeys.length - 1));
    const sidebarUp = () => setSidebarHighlight((i) => Math.max(i - 1, 0));
    sidebarKeyMap["j"] = sidebarDown;
    sidebarKeyMap["ArrowDown"] = sidebarDown;
    sidebarKeyMap["k"] = sidebarUp;
    sidebarKeyMap["ArrowUp"] = sidebarUp;
    sidebarKeyMap["Enter"] = () => selectProjectByIndex(sidebarHighlight);
    sidebarKeyMap["Escape"] = blurSidebar;
    sidebarKeyMap["l"] = blurSidebar;
    sidebarKeyMap[" "] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key) toggleCollapse(item.key);
    };
    sidebarKeyMap["ArrowRight"] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key && !item.isExpanded) toggleCollapse(item.key);
      else setSidebarFocused(false);
    };
    sidebarKeyMap["ArrowLeft"] = () => {
      const item = projectItems[sidebarHighlight];
      if (item?.hasChildren && item.key && item.isExpanded) toggleCollapse(item.key);
    };
  }

  const hasTeams = session.teams.length > 0;

  const infoBar = session.sessionPath ? (
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
  ) : undefined;

  const toolbarFor = (claudeView: ClaudeView) => (
    <ClaudeToolbar
      view={claudeView}
      hasTeams={hasTeams}
      onGoToSessions={goToPicker}
      onOpenTeams={openTeams}
      onOpenDebug={openDebug}
      onBackToList={backToList}
    />
  );

  const moveDown = () => setSelectedMessage((i) => Math.min(i + 1, session.count - 1));
  const moveUp = () => setSelectedMessage((i) => Math.max(i - 1, 0));

  const listView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: toolbarFor("list"),
    keyMap: {
      j: moveDown,
      ArrowDown: moveDown,
      k: moveUp,
      ArrowUp: moveUp,
      G: jumpToTop,
      g: jumpToBottom,
      Tab: () => toggleMessage(selectedMessage),
      Enter: () => {
        if (session.count > 0) openDetail(selectedMessage);
      },
      e: () => viewActionsRef.current.expandAll?.(),
      c: () => viewActionsRef.current.collapseAll?.(),
      t: openTeams,
      d: openDebug,
      q: goToPicker,
      Escape: goToPicker,
      s: goToPicker,
      h: focusSidebar,
      ArrowLeft: focusSidebar,
    },
    hints: claudeKeyHints("list", hasTeams),
    hintActions: {
      debug: openDebug,
      sessions: goToPicker,
      ...(hasTeams ? { tasks: openTeams } : {}),
    },
    render: () => {
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
                    session.sessionPath && void requestEfficiencyAnalysis(session.sessionPath, true)
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
            key={ctx.fontScale}
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
    },
  };

  const detailView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: toolbarFor("detail"),
    // j/k/Tab/Enter/q/Escape handled by MessageDetail's own useKeyboard
    keyMap: {},
    hints: claudeKeyHints("detail", hasTeams),
    hintActions: { back: showList },
    render: () => {
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
    },
  };

  const teamView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: toolbarFor("team"),
    keyMap: { q: showList, Escape: showList },
    hints: claudeKeyHints("team", hasTeams),
    hintActions: { back: showList },
    render: () => <TeamBoard teams={session.teams} />,
  };

  const debugView: SourceView = {
    infoBar,
    collapsible: true,
    toolbarLeft: toolbarFor("debug"),
    keyMap: { q: showList, Escape: showList },
    hints: claudeKeyHints("debug", hasTeams),
    hintActions: { back: showList },
    render: () => <DebugViewer entries={session.debugEntries} viewActionsRef={viewActionsRef} />,
  };

  return {
    picker: {
      count: picker.sessions.length,
      open: (localIndex) => {
        if (picker.sessions[localIndex]) handleSelectSession(picker.sessions[localIndex]);
      },
      headerStats: <ClaudeHeaderStats sessions={picker.sessions} />,
      render: ({ selectedIndex, onHover }) => (
        <ClaudeSessionList
          sessions={picker.sessions}
          index={picker.index}
          loading={picker.loading}
          searchQuery={ctx.searchQuery}
          selectedIndex={selectedIndex}
          onSelect={handleSelectSession}
          onHover={onHover}
          onVisiblePathsChange={picker.refresh}
          recapPreview={ctx.recapPreview}
          efficiencyJobs={efficiencyJobs.jobsBySession}
          efficiencySummaries={efficiencyJobs.summariesBySession}
          onAnalyse={(path) => void requestEfficiencyAnalysis(path, true)}
          onOpenEfficiencyDashboard={(sessionInfo) => void openEfficiencyDashboard(sessionInfo)}
        />
      ),
    },
    sidebar: {
      focused: sidebarFocused,
      focus: focusSidebar,
      blur: blurSidebar,
      keyMap: sidebarKeyMap,
      render: (style: CSSProperties) => (
        <ProjectTree
          sessions={picker.allSessions}
          selectedProject={selectedProject}
          highlightedIndex={sidebarHighlight}
          isFocused={sidebarFocused}
          collapsedKeys={collapsedKeys}
          onSelectProject={handleSelectProject}
          onToggleCollapse={toggleCollapse}
          onRefresh={loadProjectDirs}
          onFocus={focusSidebar}
          refreshing={picker.loading}
          style={style}
        />
      ),
    },
    status: <IndexProgressBar progress={picker.index} />,
    hasOpenSession: !!session.sessionPath,
    backToSession: backToList,
    onSettingsLoaded,
    onSettingsSaved: loadProjectDirs,
    views: { list: listView, detail: detailView, team: teamView, debug: debugView },
    overlays: (
      <>
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
              openSettings("analytics");
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
      </>
    ),
  };
}
