import { useState, useEffect, useCallback } from "react";
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

type ViewState = "picker" | "list" | "detail" | "team" | "debug";

export function App() {
  const { exit } = useApp();
  const [view, setView] = useState<ViewState>("picker");

  // Session state
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

  // Load a session
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

  // Live updates via SSE
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

  // Cleanup watcher on unmount
  useEffect(() => {
    return () => {
      api.unwatchSession().catch(() => {});
    };
  }, []);

  // Handle session selection
  const handleSelectSession = useCallback(
    (session: SessionInfo) => {
      loadSession(session.path);
      setView("list");
    },
    [loadSession],
  );

  // Toggle message expand
  const toggleMessage = useCallback((idx: number) => {
    setExpandedMessages((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  // Toggle detail item expand
  const toggleItem = useCallback((idx: number) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  // Keyboard handling
  useInput((input, key) => {
    switch (view) {
      case "list": {
        if (input === "j" || key.downArrow) {
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
          if (messages.length > 0 && messages[selectedMessage]?.items.length > 0) {
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
        // picker handles its own input
        break;
      }
    }
  });

  // Render
  const renderView = () => {
    switch (view) {
      case "picker":
        return <SessionPicker onSelect={handleSelectSession} onQuit={exit} />;
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
      {/* Info bar — show when session loaded and not on picker */}
      {sessionPath && view !== "picker" && (
        <InfoBar
          meta={meta}
          messages={messages}
          sessionTotals={totals}
          sessionPath={sessionPath}
          ongoing={ongoing}
        />
      )}

      {/* Main content */}
      {renderView()}

      {/* Keybind bar */}
      {view !== "picker" && (
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
      )}
    </Box>
  );
}
