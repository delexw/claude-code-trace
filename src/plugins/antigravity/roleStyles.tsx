import type { RoleStyle } from "../../components/MessageItem";
import type { AntigravityMessage } from "./types";
import icon from "./antigravity.png";

/** The Antigravity mark, sized like the other role icons. */
export function AntigravityIcon() {
  return <img src={icon} alt="" className="antigravity__icon" />;
}

/**
 * How each step type draws. `DisplayMessage.role` cannot say "tool call" or
 * "file read", so the colour and label ride along as the row's `RoleStyle`.
 */
const ROLE_STYLES: Record<AntigravityMessage["role"], RoleStyle> = {
  user: { className: "message--user", label: "User", modifier: "user" },
  assistant: {
    className: "message--claude",
    label: "Assistant",
    modifier: "claude",
    icon: <AntigravityIcon />,
  },
  tool: { className: "message--ag-tool", label: "MCP Tool", modifier: "ag-tool" },
  file: { className: "message--ag-file", label: "View File", modifier: "ag-file" },
  search: { className: "message--ag-search", label: "Grep Search", modifier: "ag-search" },
  error: { className: "message--system-error", label: "Error", modifier: "system-error" },
  checkpoint: { className: "message--compact", label: "Checkpoint", modifier: "compact" },
  context: { className: "message--ag-context", label: "Context", modifier: "ag-context" },
  system: { className: "message--system", label: "System", modifier: "system" },
};

export function roleStyle(role: AntigravityMessage["role"]): RoleStyle {
  return ROLE_STYLES[role];
}
