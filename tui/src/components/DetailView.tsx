import { Box, Text } from "ink";
import type { DisplayMessage, DisplayItem } from "../api.js";
import { formatDuration, truncate, roleColor, roleIcon, formatJson } from "../lib/format.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";

interface DetailViewProps {
  message: DisplayMessage;
  selectedItem: number;
  expandedItems: Set<number>;
  ongoing: boolean;
}

function getItemIcon(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking":
      return "💭";
    case "Output":
      return "✎";
    case "ToolCall":
      return item.tool_error ? "⚠" : "⚙";
    case "Subagent":
      return "🤖";
    case "TeammateMessage":
      return "👥";
    case "HookEvent":
      return "⚡";
    default:
      return "·";
  }
}

function getItemName(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking":
      return "Thinking";
    case "Output":
      return "Output";
    case "ToolCall":
      return item.tool_name || "Tool";
    case "Subagent":
      return item.subagent_type || "Subagent";
    case "TeammateMessage":
      return item.team_member_name || "Teammate";
    case "HookEvent":
      return item.hook_event || "Hook";
    default:
      return item.item_type;
  }
}

function getItemSummary(item: DisplayItem): string {
  switch (item.item_type) {
    case "ToolCall":
      return item.tool_summary || "";
    case "Subagent":
      return item.subagent_desc || "";
    case "TeammateMessage":
      return item.text ? item.text.slice(0, 100) : "";
    case "Thinking":
      return item.text
        ? item.text.slice(0, 80) + (item.text.length > 80 ? "…" : "")
        : "Content not recorded";
    case "Output":
      return item.text ? item.text.slice(0, 80) + (item.text.length > 80 ? "…" : "") : "";
    case "HookEvent":
      return item.hook_name
        ? `${item.hook_name}${item.hook_command ? ": " + truncate(item.hook_command, 60) : ""}`
        : item.hook_command
          ? truncate(item.hook_command, 80)
          : "";
    default:
      return "";
  }
}

function itemBorderColor(item: DisplayItem, isSelected: boolean): string {
  if (isSelected) return "blue";
  switch (item.item_type) {
    case "Thinking":
      return "gray";
    case "Output":
      return "white";
    case "ToolCall":
      return item.tool_error ? "red" : "blue";
    case "Subagent":
      return "cyan";
    case "TeammateMessage":
      return "blue";
    case "HookEvent":
      return "yellow";
    default:
      return "gray";
  }
}

function itemColor(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking":
      return "gray";
    case "Output":
      return "white";
    case "ToolCall":
      return item.tool_error ? "red" : "blue";
    case "Subagent":
      return "cyan";
    case "TeammateMessage":
      return "blue";
    case "HookEvent":
      return "yellow";
    default:
      return "gray";
  }
}

export function DetailView({ message, selectedItem, expandedItems, ongoing }: DetailViewProps) {
  const cols = process.stdout.columns || 80;
  const stats = statsFromMessage(message);

  // Each card takes ~2-4 rows
  const rowBudget = (process.stdout.rows || 24) - 10;
  const windowSize = Math.max(3, Math.floor(rowBudget / 3));

  const items = message.items;
  let start = Math.max(0, selectedItem - Math.floor(windowSize / 2));
  const end = Math.min(items.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);
  const visible = items.slice(start, end);

  return (
    <Box flexDirection="column">
      {/* Message header card */}
      <Box
        flexDirection="column"
        borderStyle="single"
        borderColor="gray"
        borderLeft={false}
        borderRight={false}
        borderTop={false}
        paddingX={1}
      >
        <Box gap={1}>
          <Text bold color={roleColor(message.role)}>
            {roleIcon(message.role)}{" "}
            {message.role === "claude" ? "Claude" : message.role === "user" ? "User" : "System"}
          </Text>
          {message.subagent_label ? <Text color="cyan">[{message.subagent_label}]</Text> : null}
          <StatsBar stats={stats} />
          {ongoing ? (
            <Text color="green" bold>
              ● active
            </Text>
          ) : null}
        </Box>
        {items.length > 0 ? (
          <Text dimColor wrap="truncate">
            {truncate(message.content, cols - 4)}
          </Text>
        ) : null}
      </Box>

      {/* Full content when no items (e.g. user messages) */}
      {items.length === 0 ? (
        <Box paddingX={1} flexDirection="column">
          <Text wrap="wrap">{message.content}</Text>
        </Box>
      ) : (
        <Box flexDirection="column" paddingX={0}>
          {visible.map((item, i) => {
            const idx = start + i;
            const isSelected = idx === selectedItem;
            const isExpanded = expandedItems.has(idx);
            const borderClr = itemBorderColor(item, isSelected);

            return (
              <Box
                key={idx}
                flexDirection="column"
                borderStyle="round"
                borderColor={borderClr}
                marginBottom={0}
              >
                {/* Item header */}
                <Box paddingX={1} gap={1}>
                  <Text bold={isSelected} color={itemColor(item)}>
                    {isExpanded ? "▼" : "▶"} {getItemIcon(item)} {getItemName(item)}
                  </Text>
                  {getItemSummary(item) ? (
                    <Text dimColor>— {truncate(getItemSummary(item), cols - 35)}</Text>
                  ) : null}
                  {item.duration_ms > 0 ? (
                    <Text dimColor>{formatDuration(item.duration_ms)}</Text>
                  ) : null}
                  {item.subagent_ongoing ? (
                    <Text color="green" bold>
                      ●
                    </Text>
                  ) : null}
                  {item.agent_id ? <Text dimColor>[{item.agent_id.slice(0, 8)}]</Text> : null}
                </Box>

                {/* Expanded body */}
                {isExpanded && <DetailItemBody item={item} cols={cols} />}
              </Box>
            );
          })}
        </Box>
      )}
    </Box>
  );
}

function DetailItemBody({ item, cols }: { item: DisplayItem; cols: number }) {
  const maxWidth = cols - 8;

  switch (item.item_type) {
    case "Thinking":
      return (
        <Box paddingX={1} flexDirection="column">
          <Text color="gray" wrap="wrap">
            {item.text || "Thinking content is not recorded in session logs."}
          </Text>
        </Box>
      );
    case "Output":
      return (
        <Box paddingX={1} flexDirection="column">
          <Text wrap="wrap">{item.text}</Text>
        </Box>
      );
    case "ToolCall":
      return (
        <Box paddingX={1} flexDirection="column">
          {item.tool_input && (
            <Box flexDirection="column">
              <Text bold dimColor>
                INPUT
              </Text>
              <Text dimColor wrap="wrap">
                {truncate(formatJson(item.tool_input), maxWidth * 5)}
              </Text>
            </Box>
          )}
          {item.tool_result && (
            <Box flexDirection="column">
              <Text bold dimColor>
                RESULT
              </Text>
              <Text color={item.tool_error ? "red" : undefined} wrap="wrap">
                {truncate(item.tool_result, maxWidth * 5)}
              </Text>
            </Box>
          )}
        </Box>
      );
    case "Subagent":
      return (
        <Box paddingX={1} flexDirection="column">
          {item.agent_id && (
            <Box gap={1}>
              <Text bold dimColor>
                ID
              </Text>
              <Text dimColor>{item.agent_id}</Text>
            </Box>
          )}
          {item.subagent_desc && (
            <Box gap={1}>
              <Text bold dimColor>
                DESC
              </Text>
              <Text>{item.subagent_desc}</Text>
            </Box>
          )}
          {item.subagent_prompt && (
            <Box flexDirection="column">
              <Text bold dimColor>
                PROMPT
              </Text>
              <Text wrap="wrap">{truncate(item.subagent_prompt, maxWidth * 3)}</Text>
            </Box>
          )}
          {item.text && (
            <Box flexDirection="column">
              <Text bold dimColor>
                CONTENT
              </Text>
              <Text wrap="wrap">{truncate(item.text, maxWidth * 3)}</Text>
            </Box>
          )}
        </Box>
      );
    case "TeammateMessage":
      return (
        <Box paddingX={1}>
          <Text wrap="wrap">{item.text}</Text>
        </Box>
      );
    case "HookEvent":
      return (
        <Box paddingX={1} flexDirection="column">
          <Box gap={1}>
            <Text bold dimColor>
              HOOK
            </Text>
            <Text>
              {item.hook_event}: {item.hook_name}
            </Text>
          </Box>
          {item.hook_command && (
            <Box gap={1}>
              <Text bold dimColor>
                CMD
              </Text>
              <Text dimColor>{item.hook_command}</Text>
            </Box>
          )}
        </Box>
      );
    default:
      return (
        <Box paddingX={1}>
          <Text wrap="wrap">{item.text}</Text>
        </Box>
      );
  }
}
