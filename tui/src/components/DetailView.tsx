import { Box, Text } from "ink";
import type { DisplayMessage, DisplayItem } from "../api.js";
import {
  formatDuration,
  formatTokens,
  truncate,
  roleColor,
  roleIcon,
  formatJson,
} from "../lib/format.js";
import { colors, getItemColor, getTeamColor } from "../lib/theme.js";
import { getItemIcon, getItemName, getItemSummary } from "../lib/items.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";
import { BrailleSpinner, OngoingDot } from "./OngoingDots.js";
import { stableWindow } from "../lib/window.js";
import {
  IconExpanded,
  IconCollapsed,
  IconBarSingle,
  IconBarDouble,
  IconSelected2,
  IconHRule,
} from "../lib/icons.js";

/** Max content width — matches Go TUI's maxContentWidth. */
const MAX_CONTENT_WIDTH = 160;

/** Limit text to maxLines while preserving newlines. Appends "…" if truncated. */
function limitLines(text: string, maxLines: number): string {
  const lines = text.split("\n");
  if (lines.length <= maxLines) return text;
  return lines.slice(0, maxLines).join("\n") + "\n…(" + (lines.length - maxLines) + " more lines)";
}

/** Clamp each line to maxWidth so bordered boxes don't overflow the terminal. */
function clampLines(text: string, maxWidth: number): string {
  return text
    .split("\n")
    .map((line) => (line.length > maxWidth ? line.slice(0, maxWidth - 1) + "…" : line))
    .join("\n");
}

interface DetailViewProps {
  message: DisplayMessage;
  selectedItem: number;
  expandedItems: Set<number>;
  ongoing: boolean;
  depth?: number;
}

function itemBorderColor(item: DisplayItem, isSelected: boolean): string {
  if (isSelected) return colors.accent;
  // Subagent items with messages get their team color
  if (item.subagent_messages.length > 0 && item.team_color) {
    return getTeamColor(item.team_color);
  }
  return getItemColor(item.item_type, !!item.tool_error);
}

export function DetailView({
  message,
  selectedItem,
  expandedItems,
  ongoing,
  depth = 0,
}: DetailViewProps) {
  const cols = process.stdout.columns || 80;
  const contentWidth = Math.min(cols, MAX_CONTENT_WIDTH);
  const stats = statsFromMessage(message);
  const items = message.items;

  // InfoBar(3) + KeybindBar(3) + header box(3) + padding = ~10 lines of chrome
  const windowSize = Math.max(3, (process.stdout.rows || 24) - 10);
  const { start, end } = stableWindow("detail", selectedItem, items.length, windowSize);
  const visible = items.slice(start, end);

  // Fixed-width name column — computed from ALL items so it doesn't shift (matches Go TUI's 12-char pad)
  const maxNameLen = Math.max(12, ...items.map((it) => getItemName(it).length));

  return (
    <Box flexDirection="column">
      {/* Message header — round border (matches Go TUI's RoundedBorder) */}
      <Box flexDirection="column" borderStyle="round" borderColor={colors.border} paddingX={2}>
        <Box gap={1}>
          {depth > 0 ? <Text dimColor>{IconSelected2.repeat(depth)} </Text> : null}
          <Text bold color={roleColor(message.role)}>
            {roleIcon(message.role)}{" "}
            {message.role === "claude" ? "Claude" : message.role === "user" ? "User" : "System"}
          </Text>
          {message.subagent_label ? (
            <Text color={colors.itemAgent}>[{message.subagent_label}]</Text>
          ) : null}
          <StatsBar stats={stats} />
          {ongoing ? <BrailleSpinner /> : null}
        </Box>
        {items.length > 0 ? (
          <Text dimColor wrap="truncate">
            {truncate(message.content, contentWidth - 8)}
          </Text>
        ) : null}
      </Box>

      {/* Full content when no items */}
      {items.length === 0 ? (
        <Box paddingX={2} flexDirection="column">
          <Text wrap="wrap">{message.content}</Text>
        </Box>
      ) : (
        <Box flexDirection="column" paddingTop={1}>
          {visible.map((item, i) => {
            const idx = start + i;
            const isSelected = idx === selectedItem;
            const isExpanded = expandedItems.has(idx);
            const clr = itemBorderColor(item, isSelected);
            const hasAgent = item.subagent_messages.length > 0;
            const teamClr = item.team_color ? getTeamColor(item.team_color) : undefined;
            const accentClr = hasAgent && teamClr ? teamClr : clr;

            // Go TUI format: {cursor} {icon} {name:<maxNameLen} {summary}  {tokens:>9} {duration:<5}
            const summaryMaxLen = contentWidth - maxNameLen - 30; // leave room for tokens + duration

            return (
              <Box
                key={`${item.item_type}-${idx}-${item.tool_name || item.agent_id || ""}`}
                flexDirection="row"
                marginBottom={0}
              >
                {/* Left accent — double bar for subagents with messages */}
                <Text color={accentClr}>{hasAgent ? IconBarDouble : IconBarSingle}</Text>
                <Box flexDirection="column" flexGrow={1} paddingLeft={1}>
                  {/* Item header row — Go TUI aligned format */}
                  <Box>
                    {/* Cursor + icon + name (fixed width) */}
                    <Text bold={isSelected} inverse={isSelected} color={accentClr}>
                      {isExpanded ? IconExpanded : IconCollapsed} {getItemIcon(item)}{" "}
                      {getItemName(item).padEnd(maxNameLen)}
                    </Text>
                    {/* Summary — "- " separator matches Go TUI */}
                    {getItemSummary(item) ? (
                      <Text dimColor> - {truncate(getItemSummary(item), summaryMaxLen)}</Text>
                    ) : null}
                    {/* Spacer */}
                    <Box flexGrow={1} />
                    {/* Right-aligned: tokens + duration */}
                    {item.token_count > 0 ? (
                      <Text dimColor>{formatTokens(item.token_count).padStart(9)}</Text>
                    ) : null}
                    {item.duration_ms > 0 ? (
                      <Text dimColor> {formatDuration(item.duration_ms).padEnd(5)}</Text>
                    ) : null}
                    {item.subagent_ongoing ? <OngoingDot /> : null}
                    {hasAgent ? (
                      <Text color={teamClr || colors.itemAgent} dimColor>
                        {" "}
                        [{item.subagent_messages.length} msg]
                      </Text>
                    ) : null}
                  </Box>

                  {/* Expanded body */}
                  {isExpanded && <DetailItemBody item={item} cols={contentWidth} />}
                </Box>
              </Box>
            );
          })}
        </Box>
      )}
    </Box>
  );
}

/**
 * Expanded item body — matches Go TUI: 4-space indent, plain text, no bordered boxes.
 * ToolCall Input/Result separated by dashed line. Section headers are bold+dim labels.
 */
const MAX_BODY_LINES = 40;
// Left accent bar(1) + paddingLeft(1) + body indent(4) = 6
const BODY_INDENT_OVERHEAD = 6;

function DetailItemBody({ item, cols }: { item: DisplayItem; cols: number }) {
  const wrapWidth = Math.max(cols - BODY_INDENT_OVERHEAD - 4, 20); // matches Go: max(width-8, 20)
  const clamp = (text: string) => clampLines(limitLines(text, MAX_BODY_LINES), wrapWidth);
  const hrule = IconHRule.repeat(Math.min(wrapWidth, 40));

  switch (item.item_type) {
    case "Thinking":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <Text color={colors.itemThinking} wrap="truncate">
            {clamp(item.text || "Thinking content is not recorded in session logs.")}
          </Text>
        </Box>
      );
    case "Output":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Box>
      );
    case "ToolCall":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          {item.tool_input && (
            <Box flexDirection="column">
              <Text bold color={colors.textSecondary}>
                Input:
              </Text>
              <Text dimColor wrap="truncate">
                {clamp(formatJson(item.tool_input))}
              </Text>
            </Box>
          )}
          {item.tool_input && item.tool_result && <Text color={colors.textMuted}>{hrule}</Text>}
          {item.tool_result && (
            <Box flexDirection="column">
              <Text bold color={item.tool_error ? colors.error : colors.textSecondary}>
                {item.tool_error ? "Error:" : "Result:"}
              </Text>
              <Text color={item.tool_error ? colors.error : undefined} wrap="truncate">
                {clamp(item.tool_result)}
              </Text>
            </Box>
          )}
        </Box>
      );
    case "Subagent":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          {item.agent_id && (
            <Box gap={1}>
              <Text color={colors.textMuted} bold>
                id:
              </Text>
              <Text dimColor>{item.agent_id}</Text>
            </Box>
          )}
          {item.subagent_desc && (
            <Box gap={1}>
              <Text color={colors.textMuted} bold>
                description:
              </Text>
              <Text wrap="truncate">{truncate(item.subagent_desc, wrapWidth - 15)}</Text>
            </Box>
          )}
          {item.subagent_prompt && (
            <Box flexDirection="column">
              <Text color={colors.textMuted} bold>
                prompt:
              </Text>
              <Text dimColor wrap="truncate">
                {clamp(item.subagent_prompt)}
              </Text>
            </Box>
          )}
          {item.text && (
            <Box flexDirection="column">
              <Text color={colors.textMuted}>{hrule}</Text>
              <Text bold color={colors.textSecondary}>
                Result:
              </Text>
              <Text wrap="truncate">{clamp(item.text)}</Text>
            </Box>
          )}
        </Box>
      );
    case "TeammateMessage":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Box>
      );
    case "HookEvent":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <Box gap={1}>
            <Text color={colors.textMuted} bold>
              hook:
            </Text>
            <Text>
              {item.hook_event}: {item.hook_name}
            </Text>
          </Box>
          {item.hook_command && (
            <Box gap={1}>
              <Text color={colors.textMuted} bold>
                cmd:
              </Text>
              <Text dimColor>{item.hook_command}</Text>
            </Box>
          )}
        </Box>
      );
    default:
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Box>
      );
  }
}
