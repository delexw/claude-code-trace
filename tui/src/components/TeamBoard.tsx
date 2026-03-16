import { Box, Text } from "ink";
import type { TeamSnapshot } from "../api.js";

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
      return "green";
    case "in_progress":
      return "yellow";
    case "pending":
      return "white";
    case "cancelled":
      return "red";
    default:
      return "white";
  }
}

function memberColor(color: string): string {
  const map: Record<string, string> = {
    blue: "blue",
    green: "green",
    red: "red",
    yellow: "yellow",
    purple: "magenta",
    cyan: "cyan",
    orange: "yellow",
    pink: "magenta",
  };
  return map[color.toLowerCase()] ?? "white";
}

export function TeamBoard({ teams }: TeamBoardProps) {
  if (teams.length === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No active teams</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      <Box marginBottom={1}>
        <Text bold>Teams ({teams.length})</Text>
      </Box>

      {teams
        .filter((t) => !t.deleted)
        .map((team) => (
          <Box key={team.name} flexDirection="column" marginBottom={1}>
            {/* Team header */}
            <Box gap={1}>
              <Text bold color="cyan">
                {team.name}
              </Text>
              {team.description ? <Text dimColor>— {team.description}</Text> : null}
            </Box>

            {/* Members */}
            <Box paddingLeft={2} gap={1}>
              <Text dimColor>Members:</Text>
              {team.members.map((m) => {
                const clr = team.member_colors[m] || "white";
                const isOngoing = team.member_ongoing[m] ?? false;
                return (
                  <Box key={m} gap={0}>
                    <Text color={memberColor(clr)}>{m}</Text>
                    {isOngoing ? (
                      <Text color="green" bold>
                        ●
                      </Text>
                    ) : null}
                  </Box>
                );
              })}
            </Box>

            {/* Tasks */}
            {team.tasks.length > 0 && (
              <Box flexDirection="column" paddingLeft={2}>
                {team.tasks.map((task) => (
                  <Box key={task.id} gap={1}>
                    <Text color={statusColor(task.status)}>{statusIcon(task.status)}</Text>
                    <Text>{task.subject}</Text>
                    {task.owner ? <Text dimColor>({task.owner})</Text> : null}
                  </Box>
                ))}
              </Box>
            )}
          </Box>
        ))}
    </Box>
  );
}
