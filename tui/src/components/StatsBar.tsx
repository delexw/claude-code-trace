import { Box, Text } from "ink";
import type { DisplayMessage } from "../api.js";
import { formatTokens, formatDuration } from "../../../shared/format.js";

export interface Stats {
  tokens: number;
  toolCount: number;
  thinkingCount: number;
  outputCount: number;
  durationMs: number;
  agentCount: number;
  spawnCount: number;
}

export function statsFromMessage(msg: DisplayMessage): Stats {
  const agentCount = msg.items.filter(
    (it) => it.item_type === "Subagent" || it.subagent_messages.length > 0,
  ).length;
  return {
    tokens: msg.tokens_raw,
    toolCount: msg.tool_call_count,
    thinkingCount: msg.thinking_count,
    outputCount: msg.output_count,
    durationMs: msg.duration_ms,
    agentCount,
    spawnCount: msg.teammate_spawns,
  };
}

function hasAny(s: Stats): boolean {
  return (
    s.tokens > 0 ||
    s.toolCount > 0 ||
    s.thinkingCount > 0 ||
    s.outputCount > 0 ||
    s.durationMs > 0 ||
    s.agentCount > 0 ||
    s.spawnCount > 0
  );
}

export function StatsBar({ stats }: { stats: Stats }) {
  if (!hasAny(stats)) return null;

  return (
    <Box gap={1}>
      {stats.tokens > 0 && (
        <Text dimColor color={stats.tokens > 150000 ? "yellow" : undefined}>
          {formatTokens(stats.tokens)} tok
        </Text>
      )}
      {stats.toolCount > 0 && (
        <Text dimColor color="blue">
          ⚙{stats.toolCount}
        </Text>
      )}
      {stats.thinkingCount > 0 && (
        <Text dimColor color="gray">
          💭{stats.thinkingCount}
        </Text>
      )}
      {stats.outputCount > 0 && <Text dimColor>✎{stats.outputCount}</Text>}
      {stats.durationMs > 0 && <Text dimColor>{formatDuration(stats.durationMs)}</Text>}
      {stats.agentCount > 0 && (
        <Text dimColor color="cyan">
          🤖{stats.agentCount}
        </Text>
      )}
      {stats.spawnCount > 0 && <Text dimColor>↗{stats.spawnCount}</Text>}
    </Box>
  );
}
