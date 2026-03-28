// Types matching the Rust backend serialization

export interface DisplayMessage {
  role: "user" | "claude" | "system" | "compact";
  model: string;
  content: string;
  timestamp: string;
  thinking_count: number;
  tool_call_count: number;
  output_count: number;
  tokens_raw: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  context_tokens: number;
  duration_ms: number;
  items: DisplayItem[];
  last_output: LastOutput | null;
  is_error: boolean;
  teammate_spawns: number;
  teammate_messages: number;
  subagent_label: string;
}

export type DisplayItemType =
  | "Thinking"
  | "Output"
  | "ToolCall"
  | "Subagent"
  | "TeammateMessage"
  | "HookEvent";

export interface DisplayItem {
  id: string;
  item_type: DisplayItemType;
  text: string;
  tool_name: string;
  tool_summary: string;
  tool_category: string;
  tool_input: string;
  tool_result: string;
  tool_error: boolean;
  duration_ms: number;
  token_count: number;
  subagent_type: string;
  subagent_desc: string;
  subagent_prompt: string;
  team_member_name: string;
  teammate_id: string;
  team_color: string;
  subagent_ongoing: boolean;
  agent_id: string;
  subagent_messages: DisplayMessage[];
  hook_event: string;
  hook_name: string;
  hook_command: string;
  is_orphan: boolean;
}

export interface LastOutput {
  output_type: "Text" | "ToolResult" | "ToolCalls";
  text: string;
  tool_name: string;
  tool_result: string;
  is_error: boolean;
  tool_calls: ToolCallSummary[];
}

export interface ToolCallSummary {
  name: string;
  summary: string;
}

export interface SessionInfo {
  path: string;
  session_id: string;
  mod_time: string;
  first_message: string;
  turn_count: number;
  is_ongoing: boolean;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd: number;
  duration_ms: number;
  model: string;
  cwd: string;
  git_branch: string;
  permission_mode: string;
}

export interface SessionMeta {
  cwd: string;
  git_branch: string;
  permission_mode: string;
}

export interface TeamSnapshot {
  name: string;
  description: string;
  tasks: TeamTask[];
  members: string[];
  member_colors: Record<string, string>;
  member_ongoing: Record<string, boolean>;
  deleted: boolean;
}

export interface TeamTask {
  id: string;
  subject: string;
  status: string;
  owner: string;
}

export interface DateGroup {
  category: string;
  sessions: SessionInfo[];
}

export interface SessionTotals {
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd: number;
  model: string;
}

export interface LoadResult {
  messages: DisplayMessage[];
  teams: TeamSnapshot[];
  ongoing: boolean;
  meta: SessionMeta;
  session_totals: SessionTotals;
}

export interface GitInfo {
  branch: string;
  dirty: boolean;
  worktree_dirs: string[];
}

export interface DebugEntry {
  timestamp: string;
  level: string;
  category: string;
  message: string;
  extra: string;
  line_num: number;
  count: number;
}

export type ViewState = "picker" | "list" | "detail" | "team" | "debug";
