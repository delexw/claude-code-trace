/** Mirrors `plugins::antigravity::BrainRoot`. */
export interface BrainRoot {
  /** `"antigravity-ide"`, `"antigravity"`, or `"custom"` for the user's own directory. */
  flavor: string;
  path: string;
  exists: boolean;
}

/** Mirrors the backend's response to `get_antigravity_settings` / `set_antigravity_root`. */
export interface AntigravitySettings {
  /** The user's override, or null when the default roots are used. */
  root: string | null;
  roots: BrainRoot[];
}

/** One conversation folder under a brain root. */
export interface AntigravityBrainFolder {
  uuid: string;
  path: string;
  mod_time_ms: number;
  title: string | null;
  /** Which root the folder was found under; the same conversation can exist under two. */
  flavor: string;
  /** False for artifact-only folders, which hold no conversation. */
  has_transcript: boolean;
}

export interface AntigravityToolCall {
  name: string;
  /** Antigravity's own label for the call (`args.toolSummary`). */
  summary: string;
  /** The whole `args` object, pretty-printed. */
  input: string;
}

/** One reader-visible step of a conversation. */
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
  /** The row's raw text; empty when the step only thought or called a tool. */
  content: string;
  created_at: string | null;
  tool_calls: AntigravityToolCall[];
  thinking_text: string | null;
  error_text: string | null;
  /** Gap to the next step, in ms; 0 when unknown. */
  duration_ms: number;
}

/** One page of a conversation plus the index describing all of it. */
export interface AntigravityWindow {
  messages: AntigravityMessage[];
  /** Reader-visible steps. */
  count: number;
  start: number;
  roles: string[];
  /** Every step in the transcript, including ones that recorded nothing. */
  total_rows: number;
}
