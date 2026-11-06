import type { DisplayMessage } from "../../types";

export interface CodexSession {
  id: string;
  path: string;
  title: string;
  cwd: string;
  model: string;
  modified_at: string;
  archived: boolean;
  source: string;
  total_tokens: number;
  cost_usd: number | null;
}

export interface CodexWindow {
  messages: DisplayMessage[];
  count: number;
  start: number;
  roles: string[];
  ongoing: boolean;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cost_usd: number | null;
  revision: string;
}

export interface CodexSettings {
  root: string | null;
  roots: { path: string; source: string; exists: boolean }[];
}
