import type { ReactNode } from "react";
import {
  VscBook,
  VscEdit,
  VscTerminalBash,
  VscSearch,
  VscChecklist,
  VscTools,
  VscGlobe,
  VscHistory,
  VscPlug,
  VscCheck,
  VscSync,
  VscCircle,
  VscAccount,
  VscTerminal,
  VscLightbulbEmpty,
  VscComment,
  VscWarning,
  VscExtensions,
  VscWatch,
  VscArrowLeft,
  VscArrowRight,
  VscLinkExternal,
  VscRefresh,
  VscTriangleRight,
  VscHubot,
  VscChevronRight,
  VscMultipleWindows,
} from "react-icons/vsc";
import { MdOutlineGeneratingTokens } from "react-icons/md";
import { FaSackDollar } from "react-icons/fa6";
import { GoGitBranch, GoGitMerge } from "react-icons/go";
import { AiOutlineRobot } from "react-icons/ai";
// Import the single icon from its subpath, not the 6139-export barrel (@thesvg/react),
// so the bundler never has to resolve the entire barrel. See LARGE_BARREL_MODULES warning.
import Claude from "@thesvg/react/claude";

// Tool category icons
export const toolCategoryIcons: Record<string, ReactNode> = {
  Read: <VscBook className="icon--read" />,
  Edit: <VscEdit className="icon--edit" />,
  Write: <VscEdit className="icon--edit" />,
  Bash: <VscTerminalBash className="icon--bash" />,
  Grep: <VscSearch className="icon--search" />,
  Glob: <VscSearch className="icon--search" />,
  Task: <VscChecklist className="icon--tool" />,
  Tool: <VscTools className="icon--tool" />,
  Web: <VscGlobe className="icon--web" />,
  Cron: <VscHistory className="icon--cron" />,
  Mcp: <VscPlug className="icon--mcp" />,
  Other: <VscTools className="icon--tool" />,
};

// Task status icons
export const taskStatusIcons: Record<string, ReactNode> = {
  completed: <VscCheck className="icon--status-done" />,
  in_progress: <VscSync className="icon--status-progress" />,
  pending: <VscCircle className="icon--status-pending" />,
};

// Role icons
export function UserIcon() {
  return <VscAccount className="icon--user" />;
}

export function ClaudeIcon({ className }: { className?: string }) {
  return <Claude className={`icon--claude ${className ?? ""}`} />;
}

export function SystemIcon() {
  return <VscTerminal className="icon--system" />;
}

// Detail item icons
export function ThinkingIcon() {
  return <VscLightbulbEmpty className="icon--thinking" />;
}

export function OutputIcon() {
  return <VscComment className="icon--output" />;
}

export function WarningIcon() {
  return <VscWarning className="icon--warning" />;
}

export function HookIcon() {
  return <VscExtensions className="icon--hook" />;
}

export function DefaultItemIcon() {
  return <VscTriangleRight />;
}

// Stats bar icons
export function TokensIcon() {
  return <MdOutlineGeneratingTokens className="icon--tokens" />;
}

export function ToolsIcon() {
  return <VscTools className="icon--tool" />;
}

export function DurationIcon() {
  return <VscWatch className="icon--duration" />;
}

export function AgentsIcon() {
  return <AiOutlineRobot className="icon--agents" />;
}

export function SpawnIcon() {
  return <VscHubot className="icon--spawn" />;
}

export function CostIcon() {
  return <FaSackDollar className="icon--cost" />;
}

// Navigation icons
export function BackIcon() {
  return <VscArrowLeft />;
}

export function ForwardIcon() {
  return <VscArrowRight />;
}

export function PopoutIcon() {
  return <VscLinkExternal />;
}

export function RefreshIcon() {
  return <VscRefresh />;
}

// Detail chevron icons
export function ChevronIcon() {
  return <VscChevronRight />;
}

export function PanelChevronIcon() {
  return <VscMultipleWindows />;
}

// Project tree icons
export function GitBranchIcon() {
  return <GoGitBranch className="icon--git" />;
}

export function GitMergeIcon() {
  return <GoGitMerge className="icon--git" />;
}
