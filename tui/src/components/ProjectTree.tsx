import { useMemo } from "react";
import { Box, Text } from "ink";
import type { SessionInfo } from "../api.js";
import { colors } from "../lib/theme.js";
import { OngoingDots } from "./OngoingDots.js";
import { buildFlatItems } from "../../../shared/projectTree.js";
import type { FlatItem } from "../../../shared/projectTree.js";

interface ProjectTreeProps {
  sessions: SessionInfo[];
  selectedProject: string | null;
  highlightedIndex: number;
  isFocused: boolean;
}

// ---- Exported hook + component ----

export type { FlatItem };

export function useProjectEntries(sessions: SessionInfo[]): FlatItem[] {
  return useMemo(() => buildFlatItems(sessions), [sessions]);
}

export function ProjectTree({
  sessions,
  selectedProject,
  highlightedIndex,
  isFocused,
}: ProjectTreeProps) {
  const entries = useProjectEntries(sessions);

  return (
    <Box
      flexDirection="column"
      borderStyle="single"
      borderColor={isFocused ? colors.accent : colors.border}
      width={26}
    >
      {/* Header */}
      <Box paddingX={1}>
        <Text bold dimColor>
          Projects
        </Text>
      </Box>

      {/* Tree items */}
      {entries.map((item, idx) => {
        const isSelected =
          !item.isGroup &&
          (item.key === selectedProject || (item.key === null && selectedProject === null));
        const isHighlighted = isFocused && idx === highlightedIndex;
        const indent = item.depth > 0 ? "  ".repeat(item.depth) : "";
        const branch = item.depth > 0 ? "└ " : "";

        return (
          <Box key={item.key ?? "__all__"} paddingX={1}>
            <Text
              inverse={isHighlighted}
              bold={isSelected}
              color={isSelected ? colors.accent : item.isGroup ? colors.textDim : undefined}
              dimColor={item.isGroup}
            >
              {isSelected && !item.isGroup ? "▸" : " "}
              {indent}
              {branch}
              {item.isGroup ? `⑃ ${item.name}` : item.name}
            </Text>
            <Text dimColor> {item.count}</Text>
            {item.ongoing ? <OngoingDots count={1} /> : null}
          </Box>
        );
      })}
    </Box>
  );
}
