import { Box, Text } from "ink";
import type { DisplayMessage } from "../api.js";
import { truncate } from "../../../shared/format.js";
import { StatsBar, statsFromMessage } from "./StatsBar.js";

interface MessageListProps {
  messages: DisplayMessage[];
  selectedIndex: number;
  expandedSet: Set<number>;
  ongoing: boolean;
}

function roleColor(role: string): string {
  switch (role) {
    case "claude":
      return "magenta";
    case "user":
      return "green";
    case "system":
      return "yellow";
    default:
      return "white";
  }
}

function roleIcon(role: string): string {
  switch (role) {
    case "claude":
      return "🤖";
    case "user":
      return "👤";
    case "system":
      return "⚙️";
    default:
      return "  ";
  }
}

function shortModel(m: string): string {
  const s = m.replace(/^claude-/, "");
  const dashIdx = s.indexOf("-");
  if (dashIdx === -1) return s;
  const family = s.slice(0, dashIdx);
  const rest = s.slice(dashIdx + 1);
  const vParts = rest.split("-");
  let version = vParts[0];
  if (vParts.length >= 2) {
    version = vParts[0] + "." + vParts[1];
  }
  return family + version;
}

function modelColor(m: string): string {
  if (m.includes("opus")) return "red";
  if (m.includes("sonnet")) return "blue";
  if (m.includes("haiku")) return "green";
  return "white";
}

function firstLine(text: string): string {
  const idx = text.indexOf("\n");
  return idx === -1 ? text : text.slice(0, idx);
}

export function MessageList({ messages, selectedIndex, expandedSet, ongoing }: MessageListProps) {
  const cols = process.stdout.columns || 80;
  const windowSize = (process.stdout.rows || 24) - 6;

  let start = Math.max(0, selectedIndex - Math.floor(windowSize / 2));
  const end = Math.min(messages.length, start + windowSize);
  if (end - start < windowSize) start = Math.max(0, end - windowSize);
  const visible = messages.slice(start, end);

  if (messages.length === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No messages loaded</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      {visible.map((msg, i) => {
        const idx = start + i;
        const isSelected = idx === selectedIndex;
        const isExpanded = expandedSet.has(idx);
        const isLast = idx === messages.length - 1;
        const contentWidth = cols - 16;
        const stats = statsFromMessage(msg);
        const model = msg.model ? shortModel(msg.model) : "";

        if (msg.role === "compact") {
          return (
            <Box key={idx} paddingX={2}>
              <Text dimColor>── {msg.content} ──</Text>
            </Box>
          );
        }

        const contentPreview = isExpanded
          ? msg.content
          : truncate(firstLine(msg.content), contentWidth - 30);

        return (
          <Box key={idx} flexDirection="column">
            {/* Header line */}
            <Box>
              <Text inverse={isSelected} bold={isSelected} color={roleColor(msg.role)}>
                {isSelected ? "▸" : " "}
                {roleIcon(msg.role)} {msg.role.padEnd(7)}
              </Text>
              {model ? (
                <Text color={modelColor(msg.model)} dimColor={!isSelected}>
                  {" "}
                  {model}
                </Text>
              ) : null}
              {msg.subagent_label ? (
                <Text color="cyan" dimColor>
                  {" "}
                  [{msg.subagent_label}]
                </Text>
              ) : null}
              {isLast && ongoing ? (
                <Text color="green" bold>
                  {" "}
                  ●
                </Text>
              ) : null}
            </Box>

            {/* Content */}
            <Box paddingLeft={3}>
              <Text dimColor={!isSelected} wrap="truncate">
                {contentPreview}
              </Text>
            </Box>

            {/* Stats */}
            {(isSelected || isExpanded) && (
              <Box paddingLeft={3}>
                <StatsBar stats={stats} />
              </Box>
            )}

            {/* Expanded items */}
            {isExpanded && msg.items.length > 0 && (
              <Box flexDirection="column" paddingLeft={5} marginBottom={1}>
                {msg.items.map((item) => (
                  <Box key={`${item.item_type}-${item.tool_name || ""}-${item.text.slice(0, 20)}`}>
                    <Text dimColor>
                      {item.item_type === "ToolCall" ? (
                        <Text color={item.tool_error ? "red" : "blue"}>
                          ⚙ {item.tool_name}
                          {item.tool_summary
                            ? ` — ${truncate(item.tool_summary, contentWidth - 20)}`
                            : ""}
                        </Text>
                      ) : item.item_type === "Thinking" ? (
                        <Text color="gray">💭 {truncate(item.text, contentWidth - 10)}</Text>
                      ) : item.item_type === "Output" ? (
                        <Text>✎ {truncate(item.text, contentWidth - 10)}</Text>
                      ) : item.item_type === "Subagent" ? (
                        <Text color="cyan">
                          🤖 {item.subagent_type || "Agent"}
                          {item.subagent_desc
                            ? ` — ${truncate(item.subagent_desc, contentWidth - 20)}`
                            : ""}
                          {item.subagent_ongoing ? " ●" : ""}
                        </Text>
                      ) : item.item_type === "TeammateMessage" ? (
                        <Text color="blue">
                          👥 {item.team_member_name || "Teammate"}:{" "}
                          {truncate(item.text, contentWidth - 20)}
                        </Text>
                      ) : item.item_type === "HookEvent" ? (
                        <Text color="yellow">
                          ⚡ {item.hook_event}: {item.hook_name}
                        </Text>
                      ) : (
                        <Text color="gray">
                          {item.item_type}: {truncate(item.text, contentWidth - 20)}
                        </Text>
                      )}
                    </Text>
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
