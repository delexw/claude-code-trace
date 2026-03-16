import { useMemo } from "react";
import { Box, Text } from "ink";
import type { DisplayMessage, SessionMeta, SessionTotals } from "../api.js";
import { formatTokens, formatCost } from "../../../shared/format.js";

interface InfoBarProps {
  meta: SessionMeta;
  messages: DisplayMessage[];
  sessionTotals: SessionTotals;
  sessionPath: string;
  ongoing: boolean;
}

function shortPath(cwd: string): string {
  if (!cwd) return "";
  const parts = cwd.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? cwd;
}

function shortMode(mode: string): string {
  switch (mode) {
    case "acceptEdits":
      return "auto-edit";
    case "bypassPermissions":
      return "yolo";
    case "plan":
      return "plan";
    default:
      return mode;
  }
}

function contextPercent(msgs: DisplayMessage[]): number {
  const contextWindowSize = 1_000_000;
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === "claude" && msgs[i].context_tokens > 0) {
      return Math.min(Math.floor((msgs[i].context_tokens * 100) / contextWindowSize), 100);
    }
  }
  return -1;
}

function contextColor(pct: number): string {
  if (pct < 50) return "green";
  if (pct < 80) return "yellow";
  return "red";
}

function modeColor(mode: string): string {
  switch (mode) {
    case "bypassPermissions":
      return "red";
    case "acceptEdits":
      return "magenta";
    case "plan":
      return "green";
    default:
      return "white";
  }
}

export function InfoBar({ meta, messages, sessionTotals, sessionPath, ongoing }: InfoBarProps) {
  const projectName = shortPath(meta.cwd);
  const sessionId = sessionPath.split("/").pop()?.replace(".jsonl", "") || "";
  const branch = meta.git_branch;
  const mode = meta.permission_mode;
  const ctxPct = useMemo(() => contextPercent(messages), [messages]);

  return (
    <Box paddingX={1} gap={1}>
      {projectName ? (
        <Text bold color="cyan">
          {projectName}
        </Text>
      ) : null}

      {sessionId ? <Text dimColor>{sessionId.slice(0, 8)}</Text> : null}

      {branch ? <Text color="magenta">{branch}</Text> : null}

      {mode && mode !== "default" ? (
        <Text color={modeColor(mode)} bold>
          [{shortMode(mode)}]
        </Text>
      ) : null}

      {ctxPct >= 0 ? <Text color={contextColor(ctxPct)}>ctx {ctxPct}%</Text> : null}

      {sessionTotals.total_tokens > 0 ? (
        <Text dimColor>{formatTokens(sessionTotals.total_tokens)} tok</Text>
      ) : null}

      {sessionTotals.cost_usd > 0 ? (
        <Text color="yellow">{formatCost(sessionTotals.cost_usd)}</Text>
      ) : null}

      {ongoing ? (
        <Text color="green" bold>
          ● active
        </Text>
      ) : null}
    </Box>
  );
}
