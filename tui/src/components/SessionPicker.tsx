import { useState, useEffect, useMemo } from "react";
import { Box, Text, useInput } from "ink";
import { Spinner } from "@inkjs/ui";
import { api, type SessionInfo } from "../api.js";
import {
  formatTokens,
  formatCost,
  formatDuration,
  timeAgo,
  truncate,
} from "../../../shared/format.js";

interface SessionPickerProps {
  onSelect: (session: SessionInfo) => void;
  onQuit: () => void;
}

function shortModel(m: string): string {
  const s = m.replace(/^claude-/, "");
  const dashIdx = s.indexOf("-");
  if (dashIdx === -1) return s;
  const family = s.slice(0, dashIdx);
  const rest = s.slice(dashIdx + 1);
  const vParts = rest.split("-");
  let version = vParts[0];
  if (vParts.length >= 2) {
    version = vParts[0] + "." + vParts[1];
  }
  return family + version;
}

function modelColor(m: string): string {
  if (m.includes("opus")) return "red";
  if (m.includes("sonnet")) return "blue";
  if (m.includes("haiku")) return "green";
  return "white";
}

interface DateGroup {
  category: string;
  items: SessionInfo[];
}

function groupByDate(items: SessionInfo[]): DateGroup[] {
  const now = new Date();
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterdayStart = new Date(todayStart.getTime() - 86400000);
  const weekStart = new Date(todayStart.getTime() - 7 * 86400000);
  const monthStart = new Date(todayStart.getTime() - 30 * 86400000);

  const groups: Record<string, SessionInfo[]> = {};
  const order = ["Today", "Yesterday", "This Week", "This Month", "Older"];

  for (const item of items) {
    const d = new Date(item.mod_time);
    let cat: string;
    if (d >= todayStart) cat = "Today";
    else if (d >= yesterdayStart) cat = "Yesterday";
    else if (d >= weekStart) cat = "This Week";
    else if (d >= monthStart) cat = "This Month";
    else cat = "Older";
    (groups[cat] ??= []).push(item);
  }

  for (const cat of order) {
    if (groups[cat]) {
      groups[cat].sort((a, b) => new Date(b.mod_time).getTime() - new Date(a.mod_time).getTime());
    }
  }

  return order
    .filter((cat) => groups[cat]?.length)
    .map((category) => ({ category, items: groups[category] }));
}

export function SessionPicker({ onSelect, onQuit }: SessionPickerProps) {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState(0);
  const [error, setError] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [searchActive, setSearchActive] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const dirs = await api.getProjectDirs();
        if (dirs.length === 0) {
          setError("No project directories found. Run the desktop app first to configure.");
          setLoading(false);
          return;
        }
        const list = await api.discoverSessions(dirs);
        setSessions(list);
      } catch (e) {
        setError(`Cannot connect to backend. Is the app running?\n${e}`);
      }
      setLoading(false);
    })();
  }, []);

  const filtered = useMemo(() => {
    if (!searchQuery) return sessions;
    const q = searchQuery.toLowerCase();
    return sessions.filter(
      (s) =>
        (s.first_message || "").toLowerCase().includes(q) ||
        s.session_id.toLowerCase().includes(q) ||
        (s.cwd || "").toLowerCase().includes(q),
    );
  }, [sessions, searchQuery]);

  const dateGroups = useMemo(() => groupByDate(filtered), [filtered]);

  // Build flat list for index tracking
  const flatList = useMemo(() => {
    const items: SessionInfo[] = [];
    for (const group of dateGroups) {
      for (const item of group.items) {
        items.push(item);
      }
    }
    return items;
  }, [dateGroups]);

  const totalTokens = useMemo(
    () => sessions.reduce((sum, s) => sum + s.total_tokens, 0),
    [sessions],
  );
  const totalCost = useMemo(() => sessions.reduce((sum, s) => sum + s.cost_usd, 0), [sessions]);

  useInput((input, key) => {
    if (searchActive) {
      if (key.escape) {
        setSearchActive(false);
        setSearchQuery("");
        return;
      }
      if (key.return) {
        setSearchActive(false);
        if (flatList.length > 0) setSelected(0);
        return;
      }
      if (key.backspace || key.delete) {
        setSearchQuery((q) => q.slice(0, -1));
        setSelected(0);
        return;
      }
      if (input && !key.ctrl && !key.meta) {
        setSearchQuery((q) => q + input);
        setSelected(0);
      }
      return;
    }

    if (loading || flatList.length === 0) {
      if (input === "q") onQuit();
      return;
    }
    if (input === "j" || key.downArrow) {
      setSelected((i) => Math.min(i + 1, flatList.length - 1));
    } else if (input === "k" || key.upArrow) {
      setSelected((i) => Math.max(i - 1, 0));
    } else if (input === "G") {
      setSelected(flatList.length - 1);
    } else if (input === "g") {
      setSelected(0);
    } else if (key.return) {
      onSelect(flatList[selected]);
    } else if (input === "/") {
      setSearchActive(true);
    } else if (input === "q") {
      onQuit();
    }
  });

  if (loading) {
    return (
      <Box padding={1}>
        <Spinner label="Discovering sessions..." />
      </Box>
    );
  }

  if (error) {
    return (
      <Box padding={1}>
        <Text color="red">{error}</Text>
      </Box>
    );
  }

  const windowSize = process.stdout.rows ? process.stdout.rows - 6 : 20;
  const half = Math.floor(windowSize / 2);
  let start = Math.max(0, selected - half);
  const end = Math.min(flatList.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);

  // Determine which items are visible
  let flatIdx = 0;
  const cols = process.stdout.columns || 80;

  return (
    <Box flexDirection="column" padding={1}>
      {/* Header */}
      <Box marginBottom={1} gap={2}>
        <Text bold>Sessions ({filtered.length})</Text>
        {totalTokens > 0 && <Text dimColor>{formatTokens(totalTokens)} tok</Text>}
        {totalCost > 0 && <Text color="yellow">{formatCost(totalCost)}</Text>}
      </Box>

      {/* Search bar */}
      {searchActive && (
        <Box marginBottom={1}>
          <Text color="blue" bold>
            / {searchQuery}
          </Text>
          <Text dimColor>█</Text>
        </Box>
      )}
      {!searchActive && searchQuery && (
        <Box marginBottom={1}>
          <Text dimColor>
            filter: "{searchQuery}" ({filtered.length} matches)
          </Text>
        </Box>
      )}

      {/* Session list with date groups */}
      {dateGroups.map((group) => {
        const groupItems = group.items.map((s) => {
          const idx = flatIdx++;
          return { session: s, idx };
        });

        // Skip groups entirely outside viewport
        const firstInGroup = groupItems[0]?.idx ?? 0;
        const lastInGroup = groupItems[groupItems.length - 1]?.idx ?? 0;
        if (lastInGroup < start || firstInGroup >= end) return null;

        return (
          <Box key={group.category} flexDirection="column">
            {/* Group header */}
            {firstInGroup >= start && firstInGroup < end && (
              <Box marginBottom={0}>
                <Text dimColor bold>
                  ─ {group.category} ─
                </Text>
              </Box>
            )}
            {groupItems.map(({ session: s, idx }) => {
              if (idx < start || idx >= end) return null;
              const isSelected = idx === selected;
              const model = s.model ? shortModel(s.model) : "";

              return (
                <Box key={s.path} flexDirection="column">
                  {/* Top line: icon + preview + active badge */}
                  <Box>
                    <Text
                      inverse={isSelected}
                      bold={isSelected}
                      color={isSelected ? "blue" : undefined}
                    >
                      {isSelected ? "▸ " : "  "}
                    </Text>
                    {s.is_ongoing && (
                      <Text color="green" bold>
                        ●{" "}
                      </Text>
                    )}
                    <Text inverse={isSelected} bold={isSelected} dimColor={!isSelected}>
                      {truncate(s.first_message || s.session_id, cols - 20)}
                    </Text>
                    {s.is_ongoing && (
                      <Text color="green" bold>
                        {" "}
                        ACTIVE
                      </Text>
                    )}
                  </Box>
                  {/* Meta line: model, turns, tokens, cost, duration, time */}
                  <Box paddingLeft={3} gap={1}>
                    {model && <Text color={modelColor(s.model)}>{model}</Text>}
                    <Text dimColor>{s.turn_count} turns</Text>
                    {s.total_tokens > 0 && <Text dimColor>{formatTokens(s.total_tokens)} tok</Text>}
                    {s.cost_usd > 0 && <Text color="yellow">{formatCost(s.cost_usd)}</Text>}
                    {s.duration_ms > 0 && <Text dimColor>{formatDuration(s.duration_ms)}</Text>}
                    <Text dimColor>{timeAgo(s.mod_time)}</Text>
                  </Box>
                </Box>
              );
            })}
          </Box>
        );
      })}

      {filtered.length === 0 && !loading && (
        <Text dimColor>{searchQuery ? "No matching sessions" : "No sessions found"}</Text>
      )}
    </Box>
  );
}
