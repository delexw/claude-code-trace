/** Mirrors `plugins::PluginStatus` on the backend. */
export interface PluginStatus {
  id: string;
  label: string;
  enabled: boolean;
  can_disable: boolean;
}

export type ApiAuthSource = "file" | "ephemeral" | "disabled";

/** Mirrors `clients::Client` on the backend. */
export interface ApiClient {
  id: string;
  name: string;
  /** Registered automatically (`web-ui`, `tui`); credential kept in a file the
   * dev server / TUI read. */
  builtin: boolean;
  /** Unix seconds. */
  created_at: number;
  issued_at: number;
  revoked_at?: number | null;
}

/** Mirrors `commands::settings::SettingsResponse` on the backend. */
export interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
  effective_dir: string;
  effective_dir_exists: boolean;
  wsl_distros: string[];
  allowed_origins: string[];
  can_focus?: boolean;
  /** Whether the HTTP API requires a registered client's credential. */
  api_auth_enabled?: boolean;
  /** Where the signing key lives: "file" (persisted), "ephemeral" (config dir
   * unusable at startup — credentials die with the process) or "disabled". */
  api_auth_source?: ApiAuthSource;
  /** The accepted clients. Credentials are never included. */
  clients?: ApiClient[];
  /** Every registered session-source plugin, in registry order. */
  plugins?: PluginStatus[];
}
