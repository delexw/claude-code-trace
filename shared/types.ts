// Types matching the Rust backend serialization

export interface DisplayMessage {
  role: "user" | "claude" | "system" | "compact" | "recap";
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
  /** All key-value pairs from the hook attachment JSON (pretty-printed). */
  hook_metadata: string;
  /** Tool result as pretty-printed JSON when the content is an object or array. */
  tool_result_json: string;
  is_orphan: boolean;
  /** True when this tool_use has no matching tool_result and the session ended (or went
   *  stale) before one arrived — e.g. a "defer" permission decision, or a SIGTERM'd
   *  print/SDK session (Claude Code 2.1.236+ no longer writes a synthetic denial). */
  is_deferred: boolean;
  /** v2.1.186+: name of the background subagent that triggered a cross-session permission prompt. */
  hook_source_agent_name: string;
  /** v2.1.186+: session UUID of the background subagent requesting permission. */
  hook_requesting_agent_uuid: string;
  /** For the advisor tool call: the model that produced the advice (e.g. "claude-opus-4-8"). */
  advisor_model: string;
  /** Why the tool call's result is a denial/interruption rather than a real outcome — e.g.
   *  "interrupted" (Claude Code 2.1.265+: process died mid-tool-call, resume keeps the
   *  original tool_use and appends this instead of rewriting the prompt), "automode-blocked",
   *  "permission-rule", "user-rejected". Empty for a genuine tool success/failure. */
  tool_denial_kind: string;
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
  /** Claude Code's end-of-session recap, when it is the session's latest entry; null otherwise. */
  recap: string | null;
  /** User-assigned session name (Claude Code `/rename`), joined from the live
   *  session registry. `null` when never named or no longer running. */
  name?: string | null;
  /** Live-session liveness from the pid-keyed registry; null when not running.
   *  `status` is an open string (Claude-Code-internal, may gain values) — the
   *  badge special-cases "busy"/"idle" and renders anything else neutrally. */
  liveness?: { status: string; idle_seconds: number; pid: number } | null;
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
  /** Every distinct working directory the session touched, in first-seen order.
   *  `dirs[0]` is the origin (where the session started); the last entry equals
   *  `cwd`. A session that never `/cd`'d holds a single entry. Optional for
   *  backward-compatibility with older cached payloads. */
  dirs?: string[];
  git_branch: string;
  permission_mode: string;
  /** The parent session this session was forked from (`/fork`), or `null` if it wasn't
   *  forked. As of Claude Code v2.1.221, a forked session gets its own worktree — a new
   *  `cwd`/project directory from its very first entry — so this id is the only reliable
   *  signal for grouping it back under its parent's project. Optional for
   *  backward-compatibility with older cached payloads. */
  forked_from_session_id?: string | null;
  /** Set when the parser finds a conversation-chain break (an unknown parent, or a
   *  fresh-conversation root after the first entry) landing at the same point the
   *  file's cwd/branch or timestamp order jumps — the signature of two unrelated
   *  sessions briefly colliding on the same filename (Claude Code < v2.1.251) and
   *  getting spliced into one file. `null`/absent when the file looks consistent. */
  integrity_warning?: string | null;
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
  /** Messages for the requested window (may be a slice of the whole session). */
  messages: DisplayMessage[];
  teams: TeamSnapshot[];
  ongoing: boolean;
  meta: SessionMeta;
  session_totals: SessionTotals;
  /** Total number of messages in the session (not just the returned window). */
  count: number;
  /** Index of the first returned message within the full session. */
  start: number;
  /** Role of every message in the full session (length === count). */
  roles: string[];
  /** Latest Claude context-window fill (tokens); 0 if none. */
  context_tokens: number;
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

export type ViewState = "picker" | "list" | "detail" | "team" | "debug" | "antigravity";

/**
 * How far the background walk has got through the project directories.
 * Mirrors `IndexProgress` in src-tauri/src/indexer.rs.
 */
export interface IndexProgress {
  files_read: number;
  total_files: number;
  bytes_read: number;
  total_bytes: number;
  done: boolean;
}

/**
 * One resolved Antigravity brain root: a flavor directory name and the
 * directory it maps to. Mirrors
 * `crate::commands::antigravity::BrainRoot`.
 */
export interface AntigravityBrainRoot {
  /** `"antigravity-ide"` (current installs), `"antigravity"` (older), or
   *  `"custom"` when the user has pointed the reader at their own directory. */
  flavor: string;
  path: string;
  exists: boolean;
}

/**
 * One Antigravity (Gemini IDE) conversation folder under
 * `~/.gemini/<flavor>/brain/<uuid>/`. Read-only envelope in the picker.
 * Mirrors `crate::commands::antigravity::AntigravityBrainFolder`.
 */
export interface AntigravityBrainFolder {
  uuid: string;
  path: string;
  mod_time_ms: number;
  title: string | null;
  /** Which flavor root this conversation was found under. A migration copies
   *  old conversations into the new root, so the same conversation can exist
   *  under both flavors; the list de-duplicates by `uuid` and keeps this
   *  label from the copy it picked. */
  flavor: string;
  /** Whether this folder holds an actual transcript. Brain roots also hold
   *  artifact folders (screenshots, reports, scratch dirs) with no
   *  conversation in them — on a real install most folders are of this kind,
   *  so the picker marks them rather than offering to open them. */
  has_transcript: boolean;
  provider_id: "antigravity";
}

/**
 * Which list the picker is browsing. `claude` and `antigravity` show one
 * source each; `all` shows both in one scroll. Selecting `antigravity` or
 * `all` requires Antigravity to be enabled in Settings.
 */
export type BrowseMode = "claude" | "antigravity" | "all";

/**
 * One normalized transcript step for an Antigravity conversation.
 * Mirrors `crate::commands::antigravity::AntigravityMessage`. The shape
 * is intentionally close to `DisplayMessage` so the same renderer can
 * draw both Claude and Antigravity sessions.
 *
 * `role` is one of:
 *  - `"user"` — a USER_INPUT turn from the user
 *  - `"assistant"` — PLANNER_RESPONSE (model output)
 *  - `"tool"` — MCP_TOOL call (payload shown raw in `content`)
 *  - `"file"` — VIEW_FILE result
 *  - `"search"` — GREP_SEARCH result
 *  - `"error"` — ERROR_MESSAGE
 *  - `"checkpoint"` — compaction marker (CHECKPOINT)
 *  - `"context"` — CONVERSATION_HISTORY, Antigravity's roll-up of recent
 *    conversations (carries `<conversation_summaries>` with ids and titles)
 *  - `"system"` — anything else; real transcripts also carry LIST_DIRECTORY,
 *    CODE_ACTION, RUN_COMMAND, INVOKE_SUBAGENT, SYSTEM_MESSAGE and GENERIC
 *
 * `KNOWLEDGE_ARTIFACTS` is the one type dropped server-side: it is pure
 * engine state with no reader value.
 *
 * `content` is the raw JSONL `content` string with no embellishment —
 * `<USER_REQUEST>` markers stay visible so the reader sees the exact
 * wire shape Antigravity produces.
 */
/** One entry of a row's `tool_calls` array. `summary` is the reader-facing
 *  label Antigravity writes in `args.toolSummary`; `input` is the whole `args`
 *  object pretty-printed — the step's real payload when `content` is empty. */
export interface AntigravityToolCall {
  name: string;
  summary: string;
  input: string;
}

export interface AntigravityMessage {
  step_index: number;
  role:
    | "user"
    | "assistant"
    | "tool"
    | "file"
    | "search"
    | "error"
    | "checkpoint"
    | "context"
    | "system";
  /** The row's raw text. Empty for ~41% of real rows — a planner turn that
   *  only thought or only called a tool writes into the fields below instead.
   *  See `body()` in `lib/antigravityToDisplay.ts`, which composes a body from
   *  whichever of the four the row actually has. */
  content: string;
  created_at: string | null;
  /** Tool calls this row issued. Real transcripts carry them on
   *  `PLANNER_RESPONSE` and `MCP_TOOL` rows alike. */
  tool_calls: AntigravityToolCall[];
  /** The row's `thinking` block, when it has one (2,931 rows of 25,064).
   *  Null for the majority, which is the normal case. */
  thinking_text: string | null;
  /** The row's `error` text (463 rows). Only `ERROR_MESSAGE` rows carry it. */
  error_text: string | null;
  /** Wall-clock gap to the next row, in ms. 0 when unknown; the stats bar
   *  hides a zero duration. */
  duration_ms: number;
}

/**
 * Bundle returned when the picker opens an Antigravity conversation.
 * Holds the original folder metadata plus the loaded message list, so
 * the viewer can render both the breadcrumb and the body.
 */
export interface AntigravitySession {
  folder: AntigravityBrainFolder;
  /** The whole conversation, one read. */
  messages: AntigravityMessage[];
  /** Reader-visible steps — what `messages` holds. */
  count: number;
  /** Every step the transcript holds, including ones that recorded nothing.
   *  Distinguishes "no transcript" from "a transcript of empty steps". */
  total_steps: number;
}

/**
 * One page of a conversation, plus the index describing all of it — the same
 * shape the Claude reader gets from `load_session`.
 *
 * `limit: 0` asks for the index alone: every role and the total count, with no
 * message body built. That is the opening request, so opening a 2.7 MB
 * transcript costs one pass and nothing else.
 */
export interface AntigravityWindow {
  /** Bodies for `[start, start + messages.length)`. Empty when `limit` is 0. */
  messages: AntigravityMessage[];
  /** Total reader-visible steps in this conversation. */
  count: number;
  /** Index of the first returned message. */
  start: number;
  /** Role of every visible step, so the list can draw placeholders before
   *  bodies land. Steps that recorded nothing are left out server-side. */
  roles: string[];
  /** Every step the transcript holds, including the ones left out. */
  total_rows: number;
}
