import { useState, useMemo } from "react";
import { Box, Text, useInput } from "ink";
import { Spinner } from "@inkjs/ui";
import type { SessionInfo } from "../api.js";
import {
  formatTokens,
  formatCost,
  formatDuration,
  timeAgo,
  truncate,
  shortModel,
  modelColor,
} from "../lib/format.js";
import { colors } from "../lib/theme.js";

interface SessionPickerProps {
  sessions: SessionInfo[];
  loading: boolean;
  error: string;
  inputDisabled?: boolean;
  onSelect: (session: SessionInfo) => void;
  onQuit: () => void;
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

export function SessionPicker({
  sessions,
  loading,
  error,
  inputDisabled,
  onSelect,
  onQuit,
}: SessionPickerProps) {
  const [selected, setSelected] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchActive, setSearchActive] = useState(false);

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
    if (inputDisabled) return;
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
        <Text color={colors.error}>{error}</Text>
      </Box>
    );
  }

  // Each card takes ~3 rows; adjust window
  const rowBudget = (process.stdout.rows || 24) - 6;
  const windowSize = Math.max(4, Math.floor(rowBudget / 3));
  const half = Math.floor(windowSize / 2);
  let start = Math.max(0, selected - half);
  const end = Math.min(flatList.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);

  let flatIdx = 0;
  const cols = process.stdout.columns || 80;

  return (
    <Box flexDirection="column">
      {/* Header bar */}
      <Box
        paddingX={1}
        gap={2}
        borderStyle="single"
        borderLeft={false}
        borderRight={false}
        borderTop={false}
        borderColor={colors.border}
      >
        <Text bold>Sessions ({filtered.length})</Text>
        {totalTokens > 0 && <Text dimColor>{formatTokens(totalTokens)} tok</Text>}
        {totalCost > 0 && <Text color={colors.tokenHigh}>{formatCost(totalCost)}</Text>}
      </Box>

      {/* Search bar */}
      {searchActive && (
        <Box paddingX={1}>
          <Text color={colors.accent} bold>
            / {searchQuery}
          </Text>
          <Text dimColor>█</Text>
        </Box>
      )}
      {!searchActive && searchQuery && (
        <Box paddingX={1}>
          <Text dimColor>
            filter: "{searchQuery}" ({filtered.length} matches)
          </Text>
        </Box>
      )}

      {/* Session cards grouped by date */}
      {dateGroups.map((group) => {
        const groupItems = group.items.map((s) => {
          const idx = flatIdx++;
          return { session: s, idx };
        });

        const firstInGroup = groupItems[0]?.idx ?? 0;
        const lastInGroup = groupItems[groupItems.length - 1]?.idx ?? 0;
        if (lastInGroup < start || firstInGroup >= end) return null;

        return (
          <Box key={group.category} flexDirection="column">
            {/* Date group header */}
            {firstInGroup >= start && firstInGroup < end && (
              <Box paddingX={1} marginTop={0}>
                <Text dimColor bold>
                  {group.category}
                </Text>
              </Box>
            )}
            {groupItems.map(({ session: s, idx }) => {
              if (idx < start || idx >= end) return null;
              const isSelected = idx === selected;
              const model = s.model ? shortModel(s.model) : "";
              const borderClr = isSelected
                ? colors.accent
                : s.is_ongoing
                  ? colors.ongoing
                  : colors.border;

              return (
                <Box key={s.path} flexDirection="row">
                  {/* Left accent border */}
                  <Text color={borderClr}>{isSelected ? "▸" : "│"}</Text>
                  <Box flexDirection="column" flexGrow={1} paddingLeft={1}>
                    {/* Top line: preview + active badge */}
                    <Box gap={1}>
                      <Text
                        bold={isSelected}
                        inverse={isSelected}
                        color={isSelected ? colors.accent : undefined}
                      >
                        {truncate(s.first_message || s.session_id, cols - 28)}
                      </Text>
                      {s.is_ongoing && (
                        <Text color={colors.ongoing} bold>
                          ● ACTIVE
                        </Text>
                      )}
                    </Box>
                    {/* Meta line */}
                    <Box gap={1}>
                      {model && <Text color={modelColor(s.model)}>{model}</Text>}
                      <Text dimColor>{s.turn_count} turns</Text>
                      {s.total_tokens > 0 && (
                        <Text dimColor>{formatTokens(s.total_tokens)} tok</Text>
                      )}
                      {s.cost_usd > 0 && (
                        <Text color={colors.tokenHigh}>{formatCost(s.cost_usd)}</Text>
                      )}
                      {s.duration_ms > 0 && <Text dimColor>{formatDuration(s.duration_ms)}</Text>}
                      <Text dimColor>{timeAgo(s.mod_time)}</Text>
                    </Box>
                  </Box>
                </Box>
              );
            })}
          </Box>
        );
      })}

      {filtered.length === 0 && !loading && (
        <Box paddingX={1}>
          <Text dimColor>{searchQuery ? "No matching sessions" : "No sessions found"}</Text>
        </Box>
      )}
    </Box>
  );
}
