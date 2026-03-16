import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Box, useApp, useInput } from "ink";
import { Spinner } from "@inkjs/ui";
import type {
  SessionInfo,
  DisplayMessage,
  TeamSnapshot,
  DebugEntry,
  SessionMeta,
  SessionTotals,
} from "./api.js";
import { api } from "./api.js";
import { useSSE } from "./useSSE.js";
import { useToggleSet } from "../../shared/hooks/useToggleSet.js";
import { SessionPicker } from "./components/SessionPicker.js";
import { MessageList } from "./components/MessageList.js";
import { DetailView } from "./components/DetailView.js";
import { TeamBoard } from "./components/TeamBoard.js";
import { DebugViewer } from "./components/DebugViewer.js";
import { InfoBar } from "./components/InfoBar.js";
import { KeybindBar } from "./components/KeybindBar.js";
import { ProjectTree, useProjectEntries } from "./components/ProjectTree.js";
import { projectKey } from "./lib/projectKey.js";

type ViewState = "picker" | "list" | "detail" | "team" | "debug";

const EMPTY_META: SessionMeta = { cwd: "", git_branch: "", permission_mode: "" };
const EMPTY_TOTALS: SessionTotals = {
  total_tokens: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_creation_tokens: 0,
  cost_usd: 0,
  model: "",
};

export function App() {
  const { exit } = useApp();
  const [view, setView] = useState<ViewState>("picker");

  // ---- Picker state ----
  const [allSessions, setAllSessions] = useState<SessionInfo[]>([]);
  const [pickerLoading, setPickerLoading] = useState(true);
  const [pickerError, setPickerError] = useState("");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [sidebarFocused, setSidebarFocused] = useState(false);
  const [sidebarHighlight, setSidebarHighlight] = useState(0);

  const projectEntries = useProjectEntries(allSessions);

  const pickerSessions = useMemo(() => {
    if (selectedProject === null) return allSessions;
    return allSessions.filter((s) => projectKey(s.path) === selectedProject);
  }, [allSessions, selectedProject]);

  useEffect(() => {
    let cancelled = false;
    const attempt = async (retries: number): Promise<void> => {
      try {
        const d = await api.getProjectDirs();
        if (cancelled) return;
        if (d.length === 0) {
          setPickerError("No project directories found. Run the desktop app first to configure.");
          setPickerLoading(false);
          return;
        }
        const list = await api.discoverSessions(d);
        if (cancelled) return;
        setAllSessions(list);
        setPickerLoading(false);
        await api.watchPicker(d);
      } catch (e) {
        if (cancelled) return;
        if (retries > 0) {
          await new Promise((r) => setTimeout(r, 1000));
          if (!cancelled) return attempt(retries - 1);
        }
        setPickerError(`Cannot connect to backend. Is the app running?\n${e}`);
        setPickerLoading(false);
      }
    };
    attempt(10);
    return () => {
      cancelled = true;
      api.unwatchPicker().catch(() => {});
    };
  }, []);

  useSSE<{ sessions: SessionInfo[] }>(
    "picker-update",
    useCallback((payload) => {
      if (payload.sessions) setAllSessions(payload.sessions);
    }, []),
  );

  // ---- Session state ----
  const [sessionPath, setSessionPath] = useState("");
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [teams, setTeams] = useState<TeamSnapshot[]>([]);
  const [ongoing, setOngoing] = useState(false);
  const [meta, setMeta] = useState<SessionMeta>(EMPTY_META);
  const [totals, setTotals] = useState<SessionTotals>(EMPTY_TOTALS);
  const [loading, setLoading] = useState(false);
  const [debugEntries, setDebugEntries] = useState<DebugEntry[]>([]);

  const expandedMessages = useToggleSet();
  const expandedItems = useToggleSet();

  const [pickerSelected, setPickerSelected] = useState(0);
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [selectedItem, setSelectedItem] = useState(0);
  const [debugSelected, setDebugSelected] = useState(0);

  const loadSession = useCallback(
    async (path: string) => {
      setLoading(true);
      try {
        const result = await api.loadSession(path);
        setSessionPath(path);
        setMessages(result.messages);
        setTeams(result.teams);
        setOngoing(result.ongoing);
        setMeta(result.meta);
        setTotals(result.session_totals);
        setSelectedMessage(result.messages.length - 1);
        expandedMessages.clear();
        await api.watchSession(path);
      } catch {
        // ignore
      }
      setLoading(false);
    },
    [expandedMessages],
  );

  useSSE<{
    messages: DisplayMessage[];
    ongoing: boolean;
    permission_mode: string;
    teams: TeamSnapshot[];
    session_totals: SessionTotals;
  }>(
    "session-update",
    useCallback((payload) => {
      setMessages(payload.messages);
      setOngoing(payload.ongoing);
      setTotals(payload.session_totals);
      if (payload.teams) setTeams(payload.teams);
      if (payload.permission_mode) {
        setMeta((m) => ({ ...m, permission_mode: payload.permission_mode }));
      }
    }, []),
  );

  useEffect(() => {
    return () => {
      api.unwatchSession().catch(() => {});
    };
  }, []);

  const handleSelectSession = useCallback(
    (s: SessionInfo) => {
      loadSession(s.path);
      setView("list");
      setSidebarFocused(false);
    },
    [loadSession],
  );

  const handleSelectProject = useCallback((key: string | null) => {
    setSelectedProject(key);
    setSidebarFocused(false);
  }, []);

  // ---- Single unified keyboard handler ----
  // Use a ref so the useInput effect never re-subscribes.
  // Ink's useInput puts inputHandler in its effect deps — if we pass a
  // new function each render, the stdin listener is removed and re-added
  // on every render, causing keystrokes to be lost in the gap.
  type Key = {
    upArrow: boolean;
    downArrow: boolean;
    leftArrow: boolean;
    rightArrow: boolean;
    return: boolean;
    escape: boolean;
    tab: boolean;
    backspace: boolean;
    delete: boolean;
    ctrl: boolean;
    shift: boolean;
    meta: boolean;
  };
  const inputHandlerRef = useRef<(input: string, key: Key) => void>(() => {});
  inputHandlerRef.current = (input: string, key: Key) => {
    // Sidebar navigation
    if (sidebarFocused && view === "picker") {
      if (input === "j" || key.downArrow) {
        setSidebarHighlight((i) => Math.min(i + 1, projectEntries.length - 1));
      } else if (input === "k" || key.upArrow) {
        setSidebarHighlight((i) => Math.max(i - 1, 0));
      } else if (key.return) {
        const entry = projectEntries[sidebarHighlight];
        if (entry && !entry.isGroup) handleSelectProject(entry.key);
      } else if (input === "l" || key.rightArrow || key.escape) {
        setSidebarFocused(false);
      } else if (input === "q") {
        exit();
      }
      return;
    }

    switch (view) {
      case "list": {
        if (input === "h" || key.leftArrow) {
          setSidebarFocused(true);
        } else if (input === "j" || key.downArrow) {
          setSelectedMessage((i) => Math.min(i + 1, messages.length - 1));
        } else if (input === "k" || key.upArrow) {
          setSelectedMessage((i) => Math.max(i - 1, 0));
        } else if (input === "G") {
          setSelectedMessage(messages.length - 1);
        } else if (input === "g") {
          setSelectedMessage(0);
        } else if (key.tab) {
          expandedMessages.toggle(selectedMessage);
        } else if (key.return) {
          if (messages.length > 0 && messages[selectedMessage]) {
            setSelectedItem(0);
            expandedItems.clear();
            setView("detail");
          }
        } else if (input === "e") {
          const indices: number[] = [];
          messages.forEach((m, i) => {
            if (m.role === "claude") indices.push(i);
          });
          expandedMessages.addAll(indices);
        } else if (input === "c") {
          expandedMessages.clear();
        } else if (input === "t") {
          if (teams.length > 0) setView("team");
        } else if (input === "d") {
          if (sessionPath) {
            api
              .getDebugLog(sessionPath)
              .then(setDebugEntries)
              .catch(() => {});
            setDebugSelected(0);
            setView("debug");
          }
        } else if (input === "q" || key.escape) {
          setView("picker");
        }
        break;
      }
      case "detail": {
        const items = messages[selectedMessage]?.items || [];
        if (input === "j" || key.downArrow) {
          setSelectedItem((i) => Math.min(i + 1, items.length - 1));
        } else if (input === "k" || key.upArrow) {
          setSelectedItem((i) => Math.max(i - 1, 0));
        } else if (key.tab || key.return) {
          expandedItems.toggle(selectedItem);
        } else if (input === "e") {
          expandedItems.addAll(items.map((_it, i) => i));
        } else if (input === "c") {
          expandedItems.clear();
        } else if (input === "q" || key.escape) {
          setView("list");
        }
        break;
      }
      case "debug": {
        if (input === "j" || key.downArrow) {
          setDebugSelected((i) => Math.min(i + 1, debugEntries.length - 1));
        } else if (input === "k" || key.upArrow) {
          setDebugSelected((i) => Math.max(i - 1, 0));
        } else if (input === "q" || key.escape) {
          setView("list");
        }
        break;
      }
      case "team": {
        if (input === "q" || key.escape) setView("list");
        break;
      }
      case "picker": {
        if (input === "h" || key.leftArrow) setSidebarFocused(true);
        else if (input === "j" || key.downArrow) {
          setPickerSelected((i) => Math.min(i + 1, pickerSessions.length - 1));
        } else if (input === "k" || key.upArrow) {
          setPickerSelected((i) => Math.max(i - 1, 0));
        } else if (input === "G") {
          setPickerSelected(pickerSessions.length - 1);
        } else if (input === "g") {
          setPickerSelected(0);
        } else if (key.return) {
          if (pickerSessions[pickerSelected]) {
            handleSelectSession(pickerSessions[pickerSelected]);
          }
        } else if (input === "q") {
          exit();
        }
        break;
      }
    }
  };
  // Stable function reference — never changes, so Ink's useInput effect never re-subscribes.
  const stableHandler = useCallback((input: string, key: Key) => {
    inputHandlerRef.current(input, key);
  }, []);
  useInput(stableHandler);

  // ---- Render ----
  const renderView = () => {
    switch (view) {
      case "picker":
        return (
          <SessionPicker
            sessions={pickerSessions}
            loading={pickerLoading}
            error={pickerError}
            selectedIndex={pickerSelected}
          />
        );
      case "list":
        if (loading) {
          return (
            <Box padding={1}>
              <Spinner label="Loading session..." />
            </Box>
          );
        }
        return (
          <MessageList
            messages={messages}
            selectedIndex={selectedMessage}
            expandedSet={expandedMessages.set}
            ongoing={ongoing}
          />
        );
      case "detail":
        if (messages[selectedMessage]) {
          return (
            <DetailView
              message={messages[selectedMessage]}
              selectedItem={selectedItem}
              expandedItems={expandedItems.set}
              ongoing={ongoing && selectedMessage === messages.length - 1}
            />
          );
        }
        return null;
      case "team":
        return <TeamBoard teams={teams} />;
      case "debug":
        return <DebugViewer entries={debugEntries} selectedIndex={debugSelected} />;
    }
  };

  const termHeight = process.stdout.rows || 24;

  return (
    <Box flexDirection="column" height={termHeight}>
      {sessionPath && view !== "picker" && (
        <InfoBar
          meta={meta}
          messages={messages}
          sessionTotals={totals}
          sessionPath={sessionPath}
          ongoing={ongoing}
        />
      )}

      <Box flexDirection="row" flexGrow={1}>
        <ProjectTree
          sessions={allSessions}
          selectedProject={selectedProject}
          highlightedIndex={sidebarHighlight}
          isFocused={sidebarFocused}
        />
        <Box flexDirection="column" flexGrow={1}>
          {renderView()}
        </Box>
      </Box>

      <KeybindBar
        view={view}
        hasTeams={teams.length > 0}
        position={
          view === "list"
            ? `${selectedMessage + 1}/${messages.length}`
            : view === "detail"
              ? `${selectedItem + 1}/${messages[selectedMessage]?.items.length || 0}`
              : view === "debug"
                ? `${debugSelected + 1}/${debugEntries.length}`
                : view === "picker"
                  ? `${pickerSelected + 1}/${pickerSessions.length}`
                  : undefined
        }
      />
    </Box>
  );
}
