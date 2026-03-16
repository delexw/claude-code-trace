import { Box, Text } from "ink";
import type { TeamSnapshot } from "../api.js";
import { colors, getTeamColor } from "../lib/theme.js";
import { OngoingDot } from "./OngoingDots.js";

interface TeamBoardProps {
  teams: TeamSnapshot[];
}

function statusIcon(status: string): string {
  switch (status.toLowerCase()) {
    case "completed":
      return "✓";
    case "in_progress":
      return "●";
    case "pending":
      return "○";
    case "cancelled":
      return "✗";
    default:
      return "·";
  }
}

function statusColor(status: string): string {
  switch (status.toLowerCase()) {
    case "completed":
      return colors.ongoing;
    case "in_progress":
      return colors.tokenHigh;
    case "pending":
      return colors.textPrimary;
    case "cancelled":
      return colors.error;
    default:
      return colors.textPrimary;
  }
}

function memberColor(color: string): string {
  return getTeamColor(color);
}

export function TeamBoard({ teams }: TeamBoardProps) {
  const cols = process.stdout.columns || 80;
  const ruleWidth = Math.min(cols - 4, 80);

  if (teams.length === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No active teams</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      {teams
        .filter((t) => !t.deleted)
        .map((team) => {
          // Section divider: "── team-name ──────" (matches Go TUI)
          const nameLen = team.name.length + 2; // " name "
          const leftDash = "── ";
          const rightDash =
            " " + "─".repeat(Math.max(4, ruleWidth - nameLen - leftDash.length - 1));

          // Summary: "N members · M/T done"
          const doneCount = team.tasks.filter((t) => t.status === "completed").length;
          const summary = `${team.members.length} members · ${doneCount}/${team.tasks.length} done`;

          return (
            <Box key={team.name} flexDirection="column" marginBottom={1}>
              {/* Section divider — Go TUI style */}
              <Box>
                <Text color={colors.textMuted}>{leftDash}</Text>
                <Text bold color={colors.itemAgent}>
                  {team.name}
                </Text>
                <Text color={colors.textMuted}>{rightDash}</Text>
              </Box>

              {/* Summary line */}
              <Box paddingLeft={2}>
                <Text dimColor>{summary}</Text>
              </Box>

              {/* Members — 2-space separation (matches Go TUI) */}
              <Box paddingLeft={2}>
                {team.members.map((m, idx) => {
                  const clr = team.member_colors[m] || "white";
                  const isOngoing = team.member_ongoing[m] ?? false;
                  return (
                    <Box key={m}>
                      {idx > 0 ? <Text>{"  "}</Text> : null}
                      <Text color={memberColor(clr)}>{m}</Text>
                      {isOngoing ? <OngoingDot /> : null}
                    </Box>
                  );
                })}
              </Box>

              {/* Tasks — Go TUI format: "  #ID  {status} {spinner?}  Subject  {Owner}" */}
              {team.tasks.length > 0 && (
                <Box flexDirection="column" paddingLeft={2}>
                  {team.tasks.map((task) => (
                    <Box key={task.id} gap={1}>
                      <Text dimColor>#{task.id}</Text>
                      <Text color={statusColor(task.status)}>{statusIcon(task.status)}</Text>
                      <Text>{task.subject}</Text>
                      <Box flexGrow={1} />
                      {task.owner ? <Text dimColor>{task.owner}</Text> : null}
                    </Box>
                  ))}
                </Box>
              )}
            </Box>
          );
        })}
    </Box>
  );
}
