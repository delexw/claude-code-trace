import { Box, Text } from "ink";
import type { DisplayMessage, DisplayItem } from "../api.js";
import {
  formatDuration,
  formatTokens,
  roleColor,
  roleIcon,
  formatJson,
  renderMarkdown,
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

interface DetailViewProps {
  message: DisplayMessage;
  selectedItem: number;
  expandedItems: Set<number>;
  ongoing: boolean;
  bodyScrollOffset: number;
  headerScrollOffset: number;
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
  bodyScrollOffset,
  headerScrollOffset,
  depth = 0,
}: DetailViewProps) {
  const cols = process.stdout.columns || 80;
  const contentWidth = Math.min(cols - 2, MAX_CONTENT_WIDTH); // -2 for outer paddingX={1}
  const stats = statsFromMessage(message);
  const items = message.items;

  // InfoBar(3) + KeybindBar(3) + header box(3) + padding = ~10 lines of chrome
  const windowSize = Math.max(3, (process.stdout.rows || 24) - 10);
  const { start, end } = stableWindow("detail", selectedItem, items.length, windowSize);
  const visible = items.slice(start, end);

  // Fixed-width name column — computed from ALL items, capped to prevent squeezing the summary
  const maxNameLen = Math.min(24, Math.max(12, ...items.map((it) => getItemName(it).length)));

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Message header — accent border when user has scrolled into the header content */}
      <Box
        flexDirection="column"
        borderStyle="round"
        borderColor={headerScrollOffset > 0 ? colors.accent : colors.border}
        paddingX={2}
      >
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
        {items.length > 0 && message.content ? (
          <HeaderContent content={message.content} scrollOffset={headerScrollOffset} />
        ) : null}
      </Box>

      {/* Full content when no items */}
      {items.length === 0 ? (
        <Box paddingX={2} flexDirection="column">
          <Text wrap="wrap">{renderMarkdown(message.content)}</Text>
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

            return (
              <Box
                key={`${item.item_type}-${idx}-${item.tool_name || item.agent_id || ""}`}
                flexDirection="row"
              >
                {/* Left accent — double bar for subagents with messages */}
                <Text color={accentClr}>{hasAgent ? IconBarDouble : IconBarSingle}</Text>
                <Box flexDirection="column" flexGrow={1} paddingLeft={1}>
                  {/* Item header row — Go TUI aligned format */}
                  <Box>
                    {/* Cursor + icon + name (fixed width) */}
                    <Text
                      bold={isSelected}
                      color={isSelected ? colors.accent : accentClr}
                      wrap="truncate"
                    >
                      {isExpanded ? IconExpanded : IconCollapsed} {getItemIcon(item)}{" "}
                      {getItemName(item).padEnd(maxNameLen)}
                    </Text>
                    {/* Summary — em dash separator matches MessageList */}
                    {getItemSummary(item) ? (
                      <Text dimColor wrap="truncate">
                        {" "}
                        {"\u2014"} {getItemSummary(item)}
                      </Text>
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

                  {/* Expanded body — only selected item gets the scroll offset */}
                  {isExpanded && (
                    <DetailItemBody
                      item={item}
                      cols={contentWidth}
                      scrollOffset={isSelected ? bodyScrollOffset : 0}
                    />
                  )}
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
 * Compact header preview of message.content.
 * Height scales with terminal: ~1/3 of available rows after chrome.
 * Each line is rendered individually with wrap="truncate" so the box height
 * stays fixed regardless of line length.
 */
function HeaderContent({ content, scrollOffset }: { content: string; scrollOffset: number }) {
  const rows = process.stdout.rows || 24;
  // InfoBar(3) + KeybindBar(2) + header borders/stats(4) + items(~6) = ~15 chrome rows
  const maxLines = Math.max(5, Math.floor((rows - 15) / 3));
  const lines = content.split("\n").filter((l) => l.trim() !== "");
  const clamped = Math.min(scrollOffset, Math.max(0, lines.length - maxLines));
  const visible = lines.slice(clamped, clamped + maxLines);
  const above = clamped;
  const below = Math.max(0, lines.length - clamped - maxLines);
  return (
    <Box flexDirection="column">
      {above > 0 ? (
        <Text dimColor>
          ↑ {above} line{above !== 1 ? "s" : ""} above
        </Text>
      ) : null}
      {visible.map((line, i) => (
        // eslint-disable-next-line react/no-array-index-key
        <Text key={`${clamped}-${i}`} dimColor wrap="truncate">
          {line}
        </Text>
      ))}
      {below > 0 ? (
        <Text dimColor>
          ↓ {below} more line{below !== 1 ? "s" : ""} · u/d to scroll
        </Text>
      ) : null}
    </Box>
  );
}

// InfoBar(3) + KeybindBar(3) + header box(5) + item rows(~4) + indicators(2) = ~17
const BODY_CHROME_ROWS = 17;

/**
 * Returns the slice of `text` visible at `scrollOffset` within the available
 * terminal height, plus above/below indicators.
 */
function viewportText(
  text: string,
  scrollOffset: number,
  rows: number,
): { visible: string; above: number; below: number } {
  const lines = text.split("\n");
  const maxLines = Math.max(5, rows - BODY_CHROME_ROWS);
  const clamped = Math.min(scrollOffset, Math.max(0, lines.length - maxLines));
  const visible = lines.slice(clamped, clamped + maxLines).join("\n");
  const above = clamped;
  const below = Math.max(0, lines.length - clamped - maxLines);
  return { visible, above, below };
}

/**
 * Expanded item body — matches Go TUI: 4-space indent, plain text, no bordered boxes.
 * ToolCall Input/Result separated by dashed line. Section headers are bold+dim labels.
 */
function DetailItemBody({
  item,
  cols,
  scrollOffset,
}: {
  item: DisplayItem;
  cols: number;
  scrollOffset: number;
}) {
  const rows = process.stdout.rows || 24;
  const hrule = IconHRule.repeat(Math.min(cols, 40));

  /** Render a scrollable text block with above/below indicators. */
  function ScrollBlock({
    text,
    color,
    dimColor,
  }: {
    text: string;
    color?: string;
    dimColor?: boolean;
  }) {
    const { visible, above, below } = viewportText(text, scrollOffset, rows);
    return (
      <Box flexDirection="column">
        {above > 0 ? (
          <Text dimColor>
            ↑ {above} line{above !== 1 ? "s" : ""} above (u to scroll up)
          </Text>
        ) : null}
        <Text color={color} dimColor={dimColor} wrap="wrap">
          {visible}
        </Text>
        {below > 0 ? (
          <Text dimColor>
            ↓ {below} more line{below !== 1 ? "s" : ""} (d to scroll down)
          </Text>
        ) : null}
      </Box>
    );
  }

  switch (item.item_type) {
    case "Thinking":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock
            text={item.text || "Thinking content is not recorded in session logs."}
            color={colors.itemThinking}
          />
        </Box>
      );
    case "Output":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock text={formatJson(item.text)} />
        </Box>
      );
    case "ToolCall": {
      // Concatenate input + hrule + result into one scrollable block
      const parts: string[] = [];
      if (item.tool_input) {
        parts.push("Input:");
        parts.push(formatJson(item.tool_input));
      }
      if (item.tool_input && item.tool_result) parts.push(hrule);
      if (item.tool_result) {
        parts.push(item.tool_error ? "Error:" : "Result:");
        parts.push(formatJson(item.tool_result));
      }
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock text={parts.join("\n")} color={item.tool_error ? colors.error : undefined} />
        </Box>
      );
    }
    case "Subagent": {
      const parts: string[] = [];
      if (item.agent_id) parts.push(`id: ${item.agent_id}`);
      if (item.subagent_desc) parts.push(`description: ${item.subagent_desc}`);
      if (item.subagent_prompt) parts.push(`prompt:\n${item.subagent_prompt}`);
      if (item.text) parts.push(`${hrule}\nResult:\n${item.text}`);
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock text={parts.join("\n")} />
        </Box>
      );
    }
    case "TeammateMessage":
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock text={item.text} />
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
          {item.hook_metadata && (
            <Box flexDirection="column" gap={1}>
              <Text color={colors.textMuted} bold>
                metadata:
              </Text>
              <Text dimColor>{item.hook_metadata}</Text>
            </Box>
          )}
        </Box>
      );
    default:
      return (
        <Box flexDirection="column" paddingLeft={4}>
          <ScrollBlock text={item.text} />
        </Box>
      );
  }
}
