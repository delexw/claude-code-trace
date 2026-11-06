import type { DisplayMessage } from "../../types";

export interface CodexSession {
  id: string;
  path: string;
  title: string;
  cwd: string;
  /** Stable original directory followed by any other recorded working directories. */
  dirs?: string[];
  forked_from_session_id?: string | null;
  model: string;
  git_branch?: string;
  permission_mode?: string;
  sandbox_policy?: string;
  started_at?: string;
  last_activity_at?: string;
  /** Recorded active turn time, excluding idle gaps; null when timing is unavailable. */
  duration_ms?: number | null;
  turn_count?: number;
  is_ongoing?: boolean;
  /** Latest request total, including cached reads; never cumulative session usage. */
  context_tokens?: number | null;
  context_window?: number | null;
  modified_at: string;
  archived: boolean;
  source: string;
  total_tokens: number;
  cost_usd: number | null;
  /** The recorded estimate is a known subtotal; some linked work is unpriced or unavailable. */
  cost_partial?: boolean;
}

export interface CodexWindow {
  messages: DisplayMessage[];
  count: number;
  start: number;
  roles: string[];
  ongoing: boolean;
  model?: string;
  git_branch?: string;
  permission_mode?: string;
  sandbox_policy?: string;
  started_at?: string;
  last_activity_at?: string;
  duration_ms?: number | null;
  turn_count?: number;
  context_tokens?: number | null;
  context_window?: number | null;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cost_usd: number | null;
  cost_partial?: boolean;
  revision: string;
}

export interface CodexSettings {
  root: string | null;
  roots: { path: string; source: string; exists: boolean }[];
}
