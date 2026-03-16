import type { ReactNode } from "react";
import { Box, Text } from "ink";
import type { DisplayMessage, DisplayItem } from "../api.js";
import { formatDuration, formatTokens, truncate, roleColor, roleIcon, formatJson } from "../lib/format.js";
import { colors, getItemColor, getTeamColor } from "../lib/theme.js";
import { getItemIcon, getItemName, getItemSummary } from "../lib/items.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";
import { BrailleSpinner, OngoingDot } from "./OngoingDots.js";
import { stableWindow } from "../lib/window.js";

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

  const windowSize = (process.stdout.rows || 24) - 8;
  const { start, end } = stableWindow("detail", selectedItem, items.length, windowSize);
  const visible = items.slice(start, end);

  // Fixed-width name column — computed from ALL items so it doesn't shift (matches Go TUI's 12-char pad)
  const maxNameLen = Math.max(12, ...items.map((it) => getItemName(it).length));

  return (
    <Box flexDirection="column">
      {/* Message header — round border (matches Go TUI's RoundedBorder) */}
      <Box
        flexDirection="column"
        borderStyle="round"
        borderColor={colors.border}
        paddingX={2}
        width={contentWidth}
      >
        <Box gap={1}>
          {depth > 0 ? <Text dimColor>{"\u25B8".repeat(depth)} </Text> : null}
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
                <Text color={accentClr}>{hasAgent ? "┃" : "│"}</Text>
                <Box flexDirection="column" flexGrow={1} paddingLeft={1}>
                  {/* Item header row — Go TUI aligned format */}
                  <Box>
                    {/* Cursor + icon + name (fixed width) */}
                    <Text
                      bold={isSelected}
                      inverse={isSelected}
                      color={accentClr}
                    >
                      {isExpanded ? "\u02C5" : "\u02C3"} {getItemIcon(item)}{" "}
                      {getItemName(item).padEnd(maxNameLen)}
                    </Text>
                    {/* Summary */}
                    {getItemSummary(item) ? (
                      <Text dimColor>
                        {" "}{truncate(getItemSummary(item), summaryMaxLen)}
                      </Text>
                    ) : null}
                    {/* Spacer */}
                    <Box flexGrow={1} />
                    {/* Right-aligned: tokens + duration */}
                    {item.token_count > 0 ? (
                      <Text dimColor>
                        {formatTokens(item.token_count).padStart(9)}
                      </Text>
                    ) : null}
                    {item.duration_ms > 0 ? (
                      <Text dimColor> {formatDuration(item.duration_ms).padEnd(5)}</Text>
                    ) : null}
                    {item.subagent_ongoing ? <OngoingDot /> : null}
                    {hasAgent ? (
                      <Text color={teamClr || colors.itemAgent} dimColor>
                        {" "}[{item.subagent_messages.length} msg]
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

/** Bordered container with a label in the top-left corner. */
function Section({
  label,
  width,
  children,
}: {
  label: string;
  width: number;
  children: ReactNode;
}) {
  return (
    <Box flexDirection="column" borderStyle="round" borderColor={colors.border} width={width}>
      <Box paddingX={1} flexDirection="column">
        <Text bold color={colors.textMuted}>
          {label}
        </Text>
        <Box marginTop={1} flexDirection="column">
          {children}
        </Box>
      </Box>
    </Box>
  );
}

/** Plain bordered container (no label). */
function Container({ width, children }: { width: number; children: ReactNode }) {
  return (
    <Box flexDirection="column" borderStyle="round" borderColor={colors.border} width={width}>
      <Box paddingX={1} flexDirection="column">
        {children}
      </Box>
    </Box>
  );
}

const MAX_BODY_LINES = 40;
// border (2) + paddingX (2) + left-accent bar + padding (2) = ~6
const BOX_OVERHEAD = 6;

function DetailItemBody({ item, cols }: { item: DisplayItem; cols: number }) {
  // Width available inside the box for text content
  const innerWidth = cols - BOX_OVERHEAD - 4; // 4 = border(2) + paddingX(2) inside the box
  const boxWidth = cols - BOX_OVERHEAD;
  const clamp = (text: string) => clampLines(limitLines(text, MAX_BODY_LINES), innerWidth);

  switch (item.item_type) {
    case "Thinking":
      return (
        <Container width={boxWidth}>
          <Text color={colors.itemThinking} wrap="truncate">
            {clamp(item.text || "Thinking content is not recorded in session logs.")}
          </Text>
        </Container>
      );
    case "Output":
      return (
        <Container width={boxWidth}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Container>
      );
    case "ToolCall":
      return (
        <Box flexDirection="column">
          {item.tool_input && (
            <Section label="INPUT" width={boxWidth}>
              <Text dimColor wrap="truncate">
                {clamp(formatJson(item.tool_input))}
              </Text>
            </Section>
          )}
          {item.tool_result && (
            <Section label="RESULT" width={boxWidth}>
              <Text color={item.tool_error ? colors.error : undefined} wrap="truncate">
                {clamp(item.tool_result)}
              </Text>
            </Section>
          )}
        </Box>
      );
    case "Subagent":
      return (
        <Box flexDirection="column">
          {(item.agent_id || item.subagent_desc) && (
            <Container width={boxWidth}>
              {item.agent_id && (
                <Box gap={1}>
                  <Text color={colors.textMuted} bold>
                    ID
                  </Text>
                  <Text dimColor>{item.agent_id}</Text>
                </Box>
              )}
              {item.subagent_desc && (
                <Box gap={1}>
                  <Text color={colors.textMuted} bold>
                    DESC
                  </Text>
                  <Text wrap="truncate">{item.subagent_desc}</Text>
                </Box>
              )}
            </Container>
          )}
          {item.subagent_prompt && (
            <Section label="PROMPT" width={boxWidth}>
              <Text wrap="truncate">{clamp(item.subagent_prompt)}</Text>
            </Section>
          )}
          {item.text && (
            <Section label="CONTENT" width={boxWidth}>
              <Text wrap="truncate">{clamp(item.text)}</Text>
            </Section>
          )}
        </Box>
      );
    case "TeammateMessage":
      return (
        <Container width={boxWidth}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Container>
      );
    case "HookEvent":
      return (
        <Container width={boxWidth}>
          <Box gap={1}>
            <Text color={colors.textMuted} bold>
              HOOK
            </Text>
            <Text>
              {item.hook_event}: {item.hook_name}
            </Text>
          </Box>
          {item.hook_command && (
            <Box gap={1}>
              <Text color={colors.textMuted} bold>
                CMD
              </Text>
              <Text dimColor>{item.hook_command}</Text>
            </Box>
          )}
        </Container>
      );
    default:
      return (
        <Container width={boxWidth}>
          <Text wrap="truncate">{clamp(item.text)}</Text>
        </Container>
      );
  }
}
