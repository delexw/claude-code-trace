import { Box, Text } from "ink";
import type { DisplayMessage } from "../api.js";
import { truncate, roleColor, roleIcon, shortModel, modelColor, firstLine } from "../lib/format.js";
import { colors, getRoleBorderColor, getItemColor } from "../lib/theme.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";
import { OngoingDots } from "./OngoingDots.js";

interface MessageListProps {
  messages: DisplayMessage[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
}

export function MessageList({ messages, selectedIndex, expandedSet, ongoing }: MessageListProps) {
  const cols = process.stdout.columns || 80;
  // Each collapsed message is ~3 lines (header + content + stats).
  // Window in item count so Ink output fits the terminal.
  const rows = process.stdout.rows || 24;
  const windowSize = Math.max(4, Math.floor((rows - 4) / 3));

  let start = Math.max(0, selectedIndex - Math.floor(windowSize / 2));
  const end = Math.min(messages.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);
  const visible = messages.slice(start, end);
  const contentWidth = Math.max(cols - 30, 40);

  if (messages.length === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No messages loaded</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      {visible.map((msg, i) => {
        const idx = start + i;
        const isSelected = idx === selectedIndex;
        const isExpanded = expandedSet.has(idx);
        const isLast = idx === messages.length - 1;
        const stats = statsFromMessage(msg);
        const model = msg.model ? shortModel(msg.model) : "";

        if (msg.role === "compact") {
          return (
            <Box key={idx} paddingX={1} justifyContent="center">
              <Text dimColor>────── {msg.content} ──────</Text>
            </Box>
          );
        }

        const contentPreview = isExpanded
          ? msg.content
          : truncate(firstLine(msg.content), contentWidth);
        const borderClr = isSelected ? colors.accent : getRoleBorderColor(msg.role, msg.is_error);

        return (
          <Box key={idx} flexDirection="row" marginBottom={0}>
            {/* Left accent border */}
            <Text color={borderClr}>│</Text>
            <Box flexDirection="column" flexGrow={1} paddingLeft={1}>
              {/* Header: selection indicator + role icon + name + model + badges */}
              <Box gap={1}>
                <Text
                  bold
                  inverse={isSelected}
                  color={isSelected ? colors.accent : roleColor(msg.role)}
                >
                  {isSelected ? "▸ " : "  "}
                  {roleIcon(msg.role)}{" "}
                  {msg.role === "claude" ? "Claude" : msg.role === "user" ? "User" : "System"}
                </Text>
                {model ? (
                  <Text color={modelColor(msg.model)} dimColor={!isSelected}>
                    {model}
                  </Text>
                ) : null}
                {msg.subagent_label ? (
                  <Text color={colors.itemAgent} dimColor>
                    [{msg.subagent_label}]
                  </Text>
                ) : null}
                {isLast && ongoing ? <OngoingDots /> : null}
                {isSelected ? (
                  <Text dimColor>
                    [{idx + 1}/{messages.length}]
                  </Text>
                ) : null}
              </Box>

              {/* Content */}
              <Box>
                <Text dimColor={!isSelected} wrap={isExpanded ? "wrap" : "truncate"}>
                  {contentPreview}
                </Text>
              </Box>

              {/* Stats bar */}
              <StatsBar stats={stats} />

              {/* Expanded items */}
              {isExpanded && msg.items.length > 0 && (
                <Box flexDirection="column" paddingLeft={2} marginTop={0}>
                  {msg.items.map((item) => (
                    <Box
                      key={`${item.item_type}-${item.tool_name || ""}-${item.text.slice(0, 20)}`}
                      flexDirection="row"
                    >
                      <Text color={colors.border}>│</Text>
                      <Box paddingLeft={1}>
                        {item.item_type === "ToolCall" ? (
                          <Text color={getItemColor("ToolCall", !!item.tool_error)}>
                            ⚙ {item.tool_name}
                            {item.tool_summary
                              ? ` — ${truncate(item.tool_summary, contentWidth - 20)}`
                              : ""}
                          </Text>
                        ) : item.item_type === "Thinking" ? (
                          <Text color={colors.itemThinking}>
                            ◆ {truncate(item.text, contentWidth - 10)}
                          </Text>
                        ) : item.item_type === "Output" ? (
                          <Text color={colors.itemOutput}>
                            ▪ {truncate(item.text, contentWidth - 10)}
                          </Text>
                        ) : item.item_type === "Subagent" ? (
                          <Text color={colors.itemAgent}>
                            ✦ {item.subagent_type || "Agent"}
                            {item.subagent_desc
                              ? ` — ${truncate(item.subagent_desc, contentWidth - 20)}`
                              : ""}
                            {item.subagent_ongoing ? " ●" : ""}
                          </Text>
                        ) : item.item_type === "TeammateMessage" ? (
                          <Text color={colors.itemTeammate}>
                            ◈ {item.team_member_name || "Teammate"}:{" "}
                            {truncate(item.text, contentWidth - 20)}
                          </Text>
                        ) : item.item_type === "HookEvent" ? (
                          <Text color={colors.itemHook}>
                            ⚡ {item.hook_event}: {item.hook_name}
                          </Text>
                        ) : (
                          <Text color={colors.textDim}>
                            {item.item_type}: {truncate(item.text, contentWidth - 20)}
                          </Text>
                        )}
                      </Box>
                    </Box>
                  ))}
                </Box>
              )}
            </Box>
          </Box>
        );
      })}
    </Box>
  );
}
