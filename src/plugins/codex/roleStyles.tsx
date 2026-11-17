import { AiOutlineRobot } from "react-icons/ai";
import type { DisplayMessage } from "../../types";
import type { RoleStyle } from "../../components/MessageItem";

export function CodexIcon() {
  return <AiOutlineRobot className="codex__icon" />;
}

const CODEX: RoleStyle = {
  className: "message--codex",
  label: "Codex",
  modifier: "codex",
  icon: <CodexIcon />,
};

export function roleStyle(message: DisplayMessage | undefined): RoleStyle | undefined {
  if (message?.role === "claude") return CODEX;
  return undefined;
}

export function roleLabel(message: DisplayMessage): string {
  if (message.role === "claude") return "Codex";
  if (message.role === "user") return "User";
  if (message.role === "compact") return "Compacted Message";
  if (message.role === "recap") return "Session Recap";
  return message.is_error ? "Error" : "System";
}
