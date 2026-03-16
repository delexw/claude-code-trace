import { Box, Text } from "ink";
import type { DebugEntry } from "../api.js";
import { truncate } from "../../../shared/format.js";

interface DebugViewerProps {
  entries: DebugEntry[];
  selectedIndex: number;
}

function levelColor(level: string): string {
  switch (level.toLowerCase()) {
    case "error":
      return "red";
    case "warn":
      return "yellow";
    case "info":
      return "blue";
    case "debug":
      return "gray";
    default:
      return "white";
  }
}

export function DebugViewer({ entries, selectedIndex }: DebugViewerProps) {
  const cols = process.stdout.columns || 80;
  const windowSize = (process.stdout.rows || 24) - 4;

  let start = Math.max(0, selectedIndex - Math.floor(windowSize / 2));
  const end = Math.min(entries.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);
  const visible = entries.slice(start, end);

  if (entries.length === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No debug entries</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      <Box marginBottom={1}>
        <Text bold>Debug Log ({entries.length})</Text>
      </Box>

      {visible.map((entry, i) => {
        const idx = start + i;
        const isSelected = idx === selectedIndex;
        const ts = entry.timestamp ? entry.timestamp.split("T")[1]?.split(".")[0] || "" : "";

        return (
          <Box key={idx}>
            <Text inverse={isSelected} bold={isSelected}>
              {isSelected ? "▸" : " "}
            </Text>
            <Text dimColor> {ts} </Text>
            <Text color={levelColor(entry.level)} bold>
              {entry.level.toUpperCase().padEnd(5)}{" "}
            </Text>
            {entry.category ? <Text color="cyan">[{entry.category}] </Text> : null}
            <Text dimColor={!isSelected}>{truncate(entry.message, cols - 30)}</Text>
            {entry.count > 1 ? <Text color="yellow"> x{entry.count}</Text> : null}
          </Box>
        );
      })}
    </Box>
  );
}
