import { Box, Text } from "ink";
import type { DisplayMessage } from "../api.js";
import { truncate, roleColor, roleIcon, shortModel, modelColor, firstLine } from "../lib/format.js";
import { colors, getRoleBorderColor, getItemColor, getTeamColor } from "../lib/theme.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";
import { BrailleSpinner } from "./OngoingDots.js";
import { stableWindow } from "../lib/window.js";

/** Max content width — matches Go TUI's maxContentWidth. */
const MAX_CONTENT_WIDTH = 160;

interface MessageListProps {
  messages: DisplayMessage[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
}

export function MessageList({ messages, selectedIndex, expandedSet, ongoing }: MessageListProps) {
  const cols = process.stdout.columns || 80;
  const contentWidth = Math.min(cols, MAX_CONTENT_WIDTH);
  // Each message card: border top(1) + header(1) + body(1) + stats(1) + border bottom(1) = 5 lines
  // Account for InfoBar(3) + KeybindBar(3) = 6 lines of chrome
  const rows = process.stdout.rows || 24;
  const windowSize = Math.max(3, Math.floor((rows - 6) / 5));
  const { start, end } = stableWindow("messages", selectedIndex, messages.length, windowSize);
  const visible = messages.slice(start, end);

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

        // System messages — centered dashed separator (matches Go TUI)
        if (msg.role === "compact") {
          return (
            <Box key={`compact-${idx}`} paddingX={1} justifyContent="center">
              <Text dimColor>────── {msg.content} ──────</Text>
            </Box>
          );
        }

        if (msg.role === "system") {
          return (
            <Box key={`sys-${idx}`} justifyContent="center" paddingX={2}>
              <Text color={colors.textMuted}>
                ── {roleIcon("system")} System · {truncate(msg.content, contentWidth - 20)} ──
              </Text>
            </Box>
          );
        }

        const bodyWidth = contentWidth - 8; // border(2) + paddingX(4) + accent bar(2)
        const contentPreview = isExpanded
          ? msg.content
          : truncate(firstLine(msg.content), bodyWidth);
        const borderClr = isSelected ? colors.accent : getRoleBorderColor(msg.role, msg.is_error);

        return (
          <Box
            key={`msg-${idx}-${msg.role}`}
            flexDirection="column"
            borderStyle="round"
            borderColor={borderClr}
            paddingX={2}
          >
            {/* Header: role icon + name + model + stats */}
            <Box gap={1}>
              <Text
                bold
                inverse={isSelected}
                color={isSelected ? colors.accent : roleColor(msg.role)}
              >
                {isSelected ? "\u25B8 " : "  "}
                {roleIcon(msg.role)} {msg.role === "claude" ? "Claude" : "User"}
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
              {isLast && ongoing ? <BrailleSpinner /> : null}
              {isSelected ? (
                <Text dimColor>
                  [{idx + 1}/{messages.length}]
                </Text>
              ) : null}
            </Box>

            {/* Body content — indented 2 spaces under header (matches Go TUI) */}
            <Box paddingLeft={2}>
              <Text dimColor={!isSelected} wrap={isExpanded ? "wrap" : "truncate"}>
                {contentPreview}
              </Text>
            </Box>

            {/* Stats row */}
            <StatsBar stats={stats} />

            {/* Expanded items tree */}
            {isExpanded && msg.items.length > 0 && (
              <Box flexDirection="column" paddingLeft={2} marginTop={0}>
                {msg.items.map((item) => (
                  <Box
                    key={`${idx}-${item.item_type}-${item.tool_name || item.agent_id || ""}-${item.duration_ms}`}
                    flexDirection="row"
                  >
                    <Text color={item.team_color ? getTeamColor(item.team_color) : colors.border}>
                      {item.subagent_messages.length > 0 ? "┃" : "│"}
                    </Text>
                    <Box paddingLeft={1}>
                      {item.item_type === "ToolCall" ? (
                        <Text color={getItemColor("ToolCall", !!item.tool_error)}>
                          ⚙ {item.tool_name}
                          {item.tool_summary
                            ? ` — ${truncate(item.tool_summary, bodyWidth - 20)}`
                            : ""}
                        </Text>
                      ) : item.item_type === "Thinking" ? (
                        <Text color={colors.itemThinking}>
                          ◆ {truncate(item.text, bodyWidth - 10)}
                        </Text>
                      ) : item.item_type === "Output" ? (
                        <Text color={colors.itemOutput}>
                          ▪ {truncate(item.text, bodyWidth - 10)}
                        </Text>
                      ) : item.item_type === "Subagent" ? (
                        <Text
                          color={item.team_color ? getTeamColor(item.team_color) : colors.itemAgent}
                        >
                          ✦ {item.subagent_type || "Agent"}
                          {item.subagent_desc
                            ? ` — ${truncate(item.subagent_desc, bodyWidth - 20)}`
                            : ""}
                          {item.subagent_ongoing ? " ●" : ""}
                          {item.subagent_messages.length > 0
                            ? ` [${item.subagent_messages.length} msg]`
                            : ""}
                        </Text>
                      ) : item.item_type === "TeammateMessage" ? (
                        <Text
                          color={
                            item.team_color ? getTeamColor(item.team_color) : colors.itemTeammate
                          }
                        >
                          ◈ {item.team_member_name || "Teammate"}:{" "}
                          {truncate(item.text, bodyWidth - 20)}
                        </Text>
                      ) : item.item_type === "HookEvent" ? (
                        <Text color={colors.itemHook}>
                          ⚡ {item.hook_event}: {item.hook_name}
                        </Text>
                      ) : (
                        <Text color={colors.textDim}>
                          {item.item_type}: {truncate(item.text, bodyWidth - 20)}
                        </Text>
                      )}
                    </Box>
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
