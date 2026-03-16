import { useState, useEffect, useCallback, useMemo } from "react";
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
import { SessionPicker } from "./components/SessionPicker.js";
import { MessageList } from "./components/MessageList.js";
import { DetailView } from "./components/DetailView.js";
import { TeamBoard } from "./components/TeamBoard.js";
import { DebugViewer } from "./components/DebugViewer.js";
import { InfoBar } from "./components/InfoBar.js";
import { KeybindBar } from "./components/KeybindBar.js";
import { ProjectTree, useProjectEntries } from "./components/ProjectTree.js";

type ViewState = "picker" | "list" | "detail" | "team" | "debug";

export function App() {
  const { exit } = useApp();
  const [view, setView] = useState<ViewState>("picker");

  // ---------- Picker / project tree state (lifted from SessionPicker) ----------
  const [allSessions, setAllSessions] = useState<SessionInfo[]>([]);
  const [pickerLoading, setPickerLoading] = useState(true);
  const [pickerError, setPickerError] = useState("");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [sidebarFocused, setSidebarFocused] = useState(false);
  const [sidebarHighlight, setSidebarHighlight] = useState(0);

  const projectEntries = useProjectEntries(allSessions);

  // Filter sessions by selected project
  const pickerSessions = useMemo(() => {
    if (selectedProject === null) return allSessions;
    return allSessions.filter((s) => {
      const match = s.path.match(/[/\\]\.claude[/\\]projects[/\\]([^/\\]+)/);
      return match ? match[1] === selectedProject : false;
    });
  }, [allSessions, selectedProject]);

  // Discover sessions on mount
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

  // Live picker updates
  useSSE<{ sessions: SessionInfo[] }>(
    "picker-update",
    useCallback((payload) => {
      if (payload.sessions) setAllSessions(payload.sessions);
    }, []),
  );

  // ---------- Session state ----------
  const [sessionPath, setSessionPath] = useState("");
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [teams, setTeams] = useState<TeamSnapshot[]>([]);
  const [ongoing, setOngoing] = useState(false);
  const [meta, setMeta] = useState<SessionMeta>({ cwd: "", git_branch: "", permission_mode: "" });
  const [totals, setTotals] = useState<SessionTotals>({
    total_tokens: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_tokens: 0,
    cache_creation_tokens: 0,
    cost_usd: 0,
    model: "",
  });
  const [loading, setLoading] = useState(false);

  // List view state
  const [selectedMessage, setSelectedMessage] = useState(0);
  const [expandedMessages, setExpandedMessages] = useState<Set<number>>(new Set());

  // Detail view state
  const [selectedItem, setSelectedItem] = useState(0);
  const [expandedItems, setExpandedItems] = useState<Set<number>>(new Set());

  // Debug view state
  const [debugEntries, setDebugEntries] = useState<DebugEntry[]>([]);
  const [debugSelected, setDebugSelected] = useState(0);

  const loadSession = useCallback(async (path: string) => {
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
      setExpandedMessages(new Set());
      await api.watchSession(path);
    } catch {
      // ignore
    }
    setLoading(false);
  }, []);

  // Live session updates
  useSSE<{
    messages: DisplayMessage[];
    ongoing: boolean;
    permission_mode: string;
    teams: TeamSnapshot[];
    session_totals: SessionTotals;
  }>(
    "session-update",
    useCallback(
      (payload) => {
        setMessages((prev) => {
          if (selectedMessage >= prev.length - 1) {
            setSelectedMessage(payload.messages.length - 1);
          }
          return payload.messages;
        });
        setOngoing(payload.ongoing);
        setTotals(payload.session_totals);
        if (payload.teams) setTeams(payload.teams);
        if (payload.permission_mode) {
          setMeta((m) => ({ ...m, permission_mode: payload.permission_mode }));
        }
      },
      [selectedMessage],
    ),
  );

  useEffect(() => {
    return () => {
      api.unwatchSession().catch(() => {});
    };
  }, []);

  const handleSelectSession = useCallback(
    (session: SessionInfo) => {
      loadSession(session.path);
      setView("list");
      setSidebarFocused(false);
    },
    [loadSession],
  );

  const handleSelectProject = useCallback((key: string | null) => {
    setSelectedProject(key);
    setSidebarFocused(false);
  }, []);

  const toggleMessage = useCallback((idx: number) => {
    setExpandedMessages((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  const toggleItem = useCallback((idx: number) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  // ---------- Keyboard ----------
  useInput((input, key) => {
    // Sidebar navigation (when focused)
    if (sidebarFocused && view === "picker") {
      if (input === "j" || key.downArrow) {
        setSidebarHighlight((i) => Math.min(i + 1, projectEntries.length - 1));
      } else if (input === "k" || key.upArrow) {
        setSidebarHighlight((i) => Math.max(i - 1, 0));
      } else if (key.return) {
        const entry = projectEntries[sidebarHighlight];
        if (entry) handleSelectProject(entry.key);
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
          toggleMessage(selectedMessage);
        } else if (key.return) {
          // Open detail for ANY message, not just ones with items
          if (messages.length > 0 && messages[selectedMessage]) {
            setSelectedItem(0);
            setExpandedItems(new Set());
            setView("detail");
          }
        } else if (input === "e") {
          const all = new Set<number>();
          messages.forEach((m, i) => {
            if (m.role === "claude") all.add(i);
          });
          setExpandedMessages(all);
        } else if (input === "c") {
          setExpandedMessages(new Set());
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
        } else if (key.tab) {
          toggleItem(selectedItem);
        } else if (input === "e") {
          const all = new Set<number>();
          items.forEach((_it, i) => all.add(i));
          setExpandedItems(all);
        } else if (input === "c") {
          setExpandedItems(new Set());
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
        if (input === "q" || key.escape) {
          setView("list");
        }
        break;
      }
      case "picker": {
        if (input === "h" || key.leftArrow) {
          setSidebarFocused(true);
        }
        // other picker keys handled by SessionPicker itself
        break;
      }
    }
  });

  // ---------- Render ----------
  const renderView = () => {
    switch (view) {
      case "picker":
        return (
          <SessionPicker
            sessions={pickerSessions}
            loading={pickerLoading}
            error={pickerError}
            onSelect={handleSelectSession}
            onQuit={exit}
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
            expandedSet={expandedMessages}
            ongoing={ongoing}
          />
        );
      case "detail":
        if (messages[selectedMessage]) {
          return (
            <DetailView
              message={messages[selectedMessage]}
              selectedItem={selectedItem}
              expandedItems={expandedItems}
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

  return (
    <Box flexDirection="column">
      {/* Info bar */}
      {sessionPath && view !== "picker" && (
        <InfoBar
          meta={meta}
          messages={messages}
          sessionTotals={totals}
          sessionPath={sessionPath}
          ongoing={ongoing}
        />
      )}

      {/* Main body: sidebar + content */}
      <Box flexGrow={1} flexDirection="row">
        {/* Project tree sidebar — always visible */}
        <ProjectTree
          sessions={allSessions}
          selectedProject={selectedProject}
          highlightedIndex={sidebarHighlight}
          isFocused={sidebarFocused}
        />

        {/* Main content */}
        <Box flexGrow={1} flexDirection="column">
          {renderView()}
        </Box>
      </Box>

      {/* Keybind bar */}
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
                : undefined
        }
      />
    </Box>
  );
}
