import { useMemo } from "react";
import { formatTokens, formatDuration } from "../lib/format";
import type { DisplayMessage, DisplayItem } from "../types";

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

export function statsFromSubagentMessages(messages: DisplayMessage[]): Stats {
  let tokens = 0;
  let toolCount = 0;
  let thinkingCount = 0;
  let outputCount = 0;
  let agentCount = 0;
  for (const m of messages) {
    tokens += m.tokens_raw;
    toolCount += m.tool_call_count;
    thinkingCount += m.thinking_count;
    outputCount += m.output_count;
    agentCount += m.items.filter(
      (it) => it.item_type === "Subagent" || it.subagent_messages.length > 0,
    ).length;
  }
  return {
    tokens,
    toolCount,
    thinkingCount,
    outputCount,
    durationMs: 0,
    agentCount,
    spawnCount: 0,
  };
}

export function useSubagentStats(item: DisplayItem): Stats | null {
  return useMemo(() => {
    if (item.subagent_messages.length === 0) return null;
    return statsFromSubagentMessages(item.subagent_messages);
  }, [item.subagent_messages]);
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
    <div className="message__stats">
      {stats.tokens > 0 && (
        <span
          className={`message__stat${stats.tokens > 150000 ? " message__stat--tokens-high" : ""}`}
        >
          <span className="message__stat-icon">{"\u{1FA99}"}</span>
          {formatTokens(stats.tokens)} tok
        </span>
      )}
      {stats.toolCount > 0 && (
        <span className="message__stat">
          <span className="message__stat-icon">{"\u{1F527}"}</span>
          {stats.toolCount} tool{stats.toolCount > 1 ? "s" : ""}
        </span>
      )}
      {stats.thinkingCount > 0 && (
        <span className="message__stat">
          <span className="message__stat-icon">{"\u{1F4A1}"}</span>
          {stats.thinkingCount} think
        </span>
      )}
      {stats.outputCount > 0 && (
        <span className="message__stat">
          <span className="message__stat-icon">{"\u{1F4AC}"}</span>
          {stats.outputCount} out
        </span>
      )}
      {stats.durationMs > 0 && (
        <span className="message__stat">
          <span className="message__stat-icon">{"\u23F1"}</span>
          {formatDuration(stats.durationMs)}
        </span>
      )}
      {stats.agentCount > 0 && (
        <span className="message__stat message__stat--agents">
          <span className="message__stat-icon">{"\u{1F9E9}"}</span>
          {stats.agentCount} agent{stats.agentCount > 1 ? "s" : ""}
        </span>
      )}
      {stats.spawnCount > 0 && (
        <span className="message__stat">
          <span className="message__stat-icon">{"\u{1F916}"}</span>
          {stats.spawnCount} spawn{stats.spawnCount > 1 ? "s" : ""}
        </span>
      )}
    </div>
  );
}
