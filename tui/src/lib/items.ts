import type { DisplayItem } from "../api.js";
import { truncate } from "./format.js";

/** Unicode icon for a DisplayItem type (matches web's Icons.tsx). */
export function getItemIcon(item: DisplayItem): string {
  switch (item.item_type) {
    case "Thinking":
      return "◆";
    case "Output":
      return "▪";
    case "ToolCall":
      return item.tool_error ? "✗" : "⚙";
    case "Subagent":
      return "✦";
    case "TeammateMessage":
      return "◈";
    case "HookEvent":
      return "⚡";
    default:
      return "·";
  }
}

/** Display name for a DisplayItem. */
export function getItemName(item: DisplayItem): string {
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

/** Short summary for a DisplayItem. */
export function getItemSummary(item: DisplayItem): string {
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
