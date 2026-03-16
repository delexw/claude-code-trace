import { useMemo } from "react";
import { Box, Text } from "ink";
import { Spinner } from "@inkjs/ui";
import type { SessionInfo } from "../api.js";
import {
  formatTokens,
  formatCost,
  timeAgo,
  truncate,
  shortModel,
  modelColor,
} from "../lib/format.js";
import { OngoingDot } from "./OngoingDots.js";
import { colors } from "../lib/theme.js";
import { stableWindow } from "../lib/window.js";

interface SessionPickerProps {
  sessions: SessionInfo[];
  loading: boolean;
  error: string;
  selectedIndex: number;
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

export function SessionPicker({ sessions, loading, error, selectedIndex }: SessionPickerProps) {
  const dateGroups = useMemo(() => groupByDate(sessions), [sessions]);

  const totalTokens = useMemo(
    () => sessions.reduce((sum, s) => sum + s.total_tokens, 0),
    [sessions],
  );
  const totalCost = useMemo(() => sessions.reduce((sum, s) => sum + s.cost_usd, 0), [sessions]);

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

  // 2 lines per session card + date headers
  const rows = process.stdout.rows || 24;
  const windowSize = Math.max(3, Math.floor((rows - 4) / 2));
  const { start, end } = stableWindow("picker", selectedIndex, sessions.length, windowSize);

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
        <Text bold>Sessions ({sessions.length})</Text>
        {totalTokens > 0 && <Text dimColor>{formatTokens(totalTokens)} tok</Text>}
        {totalCost > 0 && <Text color={colors.tokenHigh}>{formatCost(totalCost)}</Text>}
      </Box>

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
            {firstInGroup >= start && firstInGroup < end && (
              <Box paddingX={1} marginTop={1}>
                <Text dimColor bold>
                  {group.category}
                </Text>
              </Box>
            )}
            {groupItems.map(({ session: s, idx }) => {
              if (idx < start || idx >= end) return null;
              const isSelected = idx === selectedIndex;
              const model = s.model ? shortModel(s.model) : "";
              const borderClr = isSelected
                ? colors.accent
                : s.is_ongoing
                  ? colors.ongoing
                  : colors.border;
              const msgMaxLen = cols - 6;

              return (
                <Box key={s.path} flexDirection="column">
                  {/* Line 1: message */}
                  <Box>
                    <Text color={borderClr}>{isSelected ? " ▸ " : " │ "}</Text>
                    <Text bold={isSelected} color={isSelected ? colors.accent : colors.textPrimary}>
                      {truncate(s.first_message || s.session_id, msgMaxLen)}
                    </Text>
                    {s.is_ongoing && <OngoingDot />}
                  </Box>
                  {/* Line 2: metadata */}
                  <Box>
                    <Text color={borderClr}>{isSelected ? " ▸ " : " │ "}</Text>
                    {model && <Text color={modelColor(s.model)}>{model}</Text>}
                    <Text dimColor> {s.turn_count}t</Text>
                    {s.total_tokens > 0 && <Text dimColor> {formatTokens(s.total_tokens)}</Text>}
                    {s.cost_usd > 0 && (
                      <Text color={colors.tokenHigh}> {formatCost(s.cost_usd)}</Text>
                    )}
                    <Text dimColor> {timeAgo(s.mod_time)}</Text>
                  </Box>
                </Box>
              );
            })}
          </Box>
        );
      })}

      {sessions.length === 0 && !loading && (
        <Box paddingX={1}>
          <Text dimColor>No sessions found</Text>
        </Box>
      )}
    </Box>
  );
}
