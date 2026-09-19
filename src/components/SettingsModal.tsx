import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import { setApiToken as setLiveApiToken } from "../lib/apiToken";
import { PopoutModal } from "./PopoutModal";
import { FONT_SCALE_PRESETS, formatFontScale } from "../lib/fontScale";
import { AnalyticsSettings } from "./AnalyticsSettings";
import { BetaBadge } from "./BetaBadge";

interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
  effective_dir: string;
  effective_dir_exists: boolean;
  wsl_distros: string[];
  allowed_origins: string[];
  /** Whether the HTTP API requires a registered client's credential. */
  api_auth_enabled?: boolean;
  /** Where the signing key lives: "file" (persisted), "ephemeral" (config dir
   * unusable at startup — credentials die with the process) or "disabled". */
  api_auth_source?: ApiAuthSource;
  /** The accepted clients. Credentials are never included. */
  clients?: ApiClient[];
}

type ApiAuthSource = "file" | "ephemeral" | "disabled";

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

/** `POST /api/clients` and `.../reissue` reply: the credential is returned
 * exactly once and never stored by the backend. */
interface IssuedCredential {
  client: ApiClient;
  credential: string;
}

/** Name of the built-in client this browser UI runs as. */
const WEB_UI_CLIENT = "web-ui";

type PendingAction = { kind: "reissue" | "revoke"; id: string };

/** Named month, so the result never reads as either day/month or month/day. */
function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

interface SettingsModalProps {
  onClose: () => void;
  onSaved: () => void;
  /** Current global UI zoom level (1 = 100%). */
  fontScale: number;
  /** Apply a new zoom level immediately (also persisted by the caller). */
  onFontScaleChange: (scale: number) => void;
  /** Whether a session's recap replaces its list preview when it's the latest entry. */
  recapPreview: boolean;
  /** Toggle recap preview (persisted by the caller). */
  onRecapPreviewChange: (on: boolean) => void;
  initialTab?: SettingsTab;
}

type SettingsTab = "general" | "appearance" | "api" | "analytics";

/** Merge detected distros with already-configured ones so configured-but-offline
 * distros still appear (and stay toggleable) even when WSL isn't reporting them. */
function mergeDistros(available: string[], configured: string[]): string[] {
  const seen = new Set(available);
  return [...available, ...configured.filter((d) => !seen.has(d))];
}

/** Turn textarea contents into a trimmed, non-empty origin list. Splits on
 * newlines or commas so pasting the `CCTRACE_ALLOWED_ORIGINS` env var's own
 * comma-separated format works too. Validation itself is the backend's job. */
function parseOrigins(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((o) => o.trim())
    .filter((o) => o.length > 0);
}

export function SettingsModal({
  onClose,
  onSaved,
  fontScale,
  onFontScaleChange,
  recapPreview,
  onRecapPreviewChange,
  initialTab = "general",
}: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [projectsDir, setProjectsDir] = useState("");
  const [defaultDir, setDefaultDir] = useState("");
  const [effectiveDir, setEffectiveDir] = useState("");
  const [effectiveDirExists, setEffectiveDirExists] = useState(true);
  const [availableDistros, setAvailableDistros] = useState<string[]>([]);
  const [selectedDistros, setSelectedDistros] = useState<Set<string>>(new Set());
  const [allowedOriginsText, setAllowedOriginsText] = useState("");
  const [authSource, setAuthSource] = useState<ApiAuthSource>("file");
  const [clients, setClients] = useState<ApiClient[]>([]);
  const [newClientName, setNewClientName] = useState("");
  const [issued, setIssued] = useState<IssuedCredential | null>(null);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [busyClient, setBusyClient] = useState<string | null>(null);
  const [clientNotice, setClientNotice] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const applyResponse = useCallback((res: SettingsResponse) => {
    setDefaultDir(res.default_dir);
    setProjectsDir(res.projects_dir ?? "");
    setEffectiveDir(res.effective_dir);
    setEffectiveDirExists(res.effective_dir_exists);
    setSelectedDistros(new Set(res.wsl_distros ?? []));
    setAllowedOriginsText((res.allowed_origins ?? []).join("\n"));
    setAuthSource(res.api_auth_source ?? (res.api_auth_enabled === false ? "disabled" : "file"));
    setClients(res.clients ?? []);
  }, []);

  useEffect(() => {
    const load = async () => {
      try {
        const res = await invoke<SettingsResponse>("get_settings");
        applyResponse(res);
      } catch (err) {
        console.error("Failed to load settings:", err);
      }
      try {
        const distros = await invoke<string[]>("list_wsl_distros");
        setAvailableDistros(distros ?? []);
      } catch (err) {
        console.error("Failed to list WSL distros:", err);
      }
    };
    void load();
  }, [applyResponse]);

  const toggleDistro = useCallback((name: string) => {
    setSelectedDistros((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const dirRes = await invoke<SettingsResponse>("set_projects_dir", {
        path: projectsDir.trim() || null,
      });
      const wslRes = await invoke<SettingsResponse>("set_wsl_distros", {
        distros: [...selectedDistros],
      });
      const originsRes = await invoke<SettingsResponse>("set_allowed_origins", {
        origins: parseOrigins(allowedOriginsText),
      });
      applyResponse(originsRes ?? wslRes ?? dirRes);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [projectsDir, selectedDistros, allowedOriginsText, applyResponse, onSaved, onClose]);

  const handleReset = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const res = await invoke<SettingsResponse>("set_projects_dir", { path: null });
      applyResponse(res);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [applyResponse, onSaved, onClose]);

  const replaceClient = useCallback((client: ApiClient) => {
    setClients((prev) => prev.map((c) => (c.id === client.id ? client : c)));
  }, []);

  const handleCopyCredential = useCallback(async () => {
    if (!issued) return;
    try {
      await navigator.clipboard.writeText(issued.credential);
      setClientNotice("Credential copied to clipboard.");
    } catch {
      setClientNotice(
        "Could not access the clipboard — select the credential and copy it manually.",
      );
    }
  }, [issued]);

  const handleRegister = useCallback(async () => {
    const name = newClientName.trim();
    // One client action at a time: a second Enter while the first request is
    // in flight would register the same name twice (and error).
    if (!name || busyClient) return;
    setBusyClient("new");
    setError("");
    setPending(null);
    try {
      const res = await invoke<IssuedCredential>("register_client", { name });
      setClients((prev) => [...prev, res.client]);
      setIssued(res);
      setNewClientName("");
      setClientNotice(
        `Client "${res.client.name}" registered. Copy its credential now — it is shown once and not kept.`,
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyClient(null);
    }
  }, [newClientName, busyClient]);

  // Reissue and revoke are two-click confirms: the first click arms the
  // button, the second acts. Either one cuts off a running client, so neither
  // should be one accidental click away.
  const handleReissue = useCallback(
    async (client: ApiClient) => {
      if (pending?.kind !== "reissue" || pending.id !== client.id) {
        setPending({ kind: "reissue", id: client.id });
        setClientNotice(
          `Click again to confirm. Every credential "${client.name}" holds now will stop working.`,
        );
        return;
      }
      if (busyClient) return; // one client action in flight at a time
      setPending(null);
      setBusyClient(client.id);
      setError("");
      try {
        const res = await invoke<IssuedCredential>("reissue_client", { id: client.id });
        replaceClient(res.client);
        setIssued(res);
        if (res.client.name === WEB_UI_CLIENT) {
          // Keep this tab working: send the new credential from now on.
          // lib/listen.ts subscribes to this change and reopens the SSE
          // stream, which was authenticated with the old one. (Same-origin
          // tabs also received it as the cookie on this response; dev tabs get
          // it again over HMR when the plugin sees the rewritten file.)
          setLiveApiToken(res.credential);
        }
        setClientNotice(
          res.client.builtin
            ? `"${res.client.name}" reissued — its credential file was rewritten, so the bundled ${
                res.client.name === WEB_UI_CLIENT ? "web UI" : "TUI"
              } follows automatically.`
            : `"${res.client.name}" reissued. Copy the new credential now — it is shown once and not kept.`,
        );
      } catch (err) {
        setError(String(err));
      } finally {
        setBusyClient(null);
      }
    },
    [pending, busyClient, replaceClient],
  );

  const handleRevoke = useCallback(
    async (client: ApiClient) => {
      if (pending?.kind !== "revoke" || pending.id !== client.id) {
        setPending({ kind: "revoke", id: client.id });
        setClientNotice(
          client.name === WEB_UI_CLIENT
            ? "Click again to confirm. Revoking web-ui locks every browser tab — including this one — out of the API until it is reissued from the desktop app or another client."
            : `Click again to confirm. "${client.name}" will be rejected immediately.`,
        );
        return;
      }
      if (busyClient) return; // one client action in flight at a time
      setPending(null);
      setBusyClient(client.id);
      setError("");
      try {
        const res = await invoke<ApiClient>("revoke_client", { id: client.id });
        replaceClient(res);
        if (issued?.client.id === res.id) setIssued(null);
        setClientNotice(`"${res.name}" revoked.`);
      } catch (err) {
        setError(String(err));
      } finally {
        setBusyClient(null);
      }
    },
    [pending, busyClient, replaceClient, issued],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        void handleSave();
      }
    },
    [handleSave],
  );

  const distros = mergeDistros(availableDistros, [...selectedDistros]);

  return (
    <PopoutModal
      onClose={onClose}
      header={<span className="settings-modal__title">Settings</span>}
      initialWidth={520}
      initialHeight={560}
    >
      <div className="settings-modal">
        <div className="settings-modal__tabs" role="tablist" aria-label="Settings sections">
          {(["general", "appearance", "api", "analytics"] as SettingsTab[]).map((tab) => (
            <button
              key={tab}
              type="button"
              role="tab"
              aria-selected={activeTab === tab}
              className={`settings-modal__tab${activeTab === tab ? " settings-modal__tab--active" : ""}`}
              onClick={() => setActiveTab(tab)}
            >
              {tab[0].toUpperCase() + tab.slice(1)} {tab === "analytics" && <BetaBadge />}
            </button>
          ))}
        </div>
        {activeTab === "general" && (
          <>
            <label className="settings-modal__label" htmlFor="projects-dir">
              Projects Directory
            </label>
            <input
              id="projects-dir"
              className="settings-modal__input"
              type="text"
              value={projectsDir}
              onChange={(e) => {
                setProjectsDir(e.target.value);
                setError("");
              }}
              onKeyDown={handleKeyDown}
              placeholder={defaultDir + " (default)"}
              spellCheck={false}
              autoFocus
            />
            <p className="settings-modal__hint">Default: {defaultDir}</p>
            {effectiveDir && (
              <p
                className={
                  effectiveDirExists
                    ? "settings-modal__hint settings-modal__hint--effective"
                    : "settings-modal__hint settings-modal__hint--missing"
                }
              >
                {effectiveDirExists ? "✓ Active:" : "✗ Not found:"} {effectiveDir}
              </p>
            )}

            <label className="settings-modal__label settings-modal__label--section">
              WSL Distros
            </label>
            {distros.length === 0 ? (
              <p className="settings-modal__hint">
                No WSL distributions detected. Sessions created inside WSL appear here once a distro
                is installed.
              </p>
            ) : (
              <>
                <p className="settings-modal__hint">
                  Include projects from Claude Code running inside these distributions.
                </p>
                <div className="settings-modal__wsl">
                  {distros.map((name) => (
                    <label key={name} className="settings-modal__wsl-item">
                      <input
                        type="checkbox"
                        checked={selectedDistros.has(name)}
                        onChange={() => toggleDistro(name)}
                      />
                      <span>{name}</span>
                    </label>
                  ))}
                </div>
              </>
            )}
          </>
        )}

        {(activeTab === "general" || activeTab === "api") && (
          <>
            <label
              className="settings-modal__label settings-modal__label--section"
              htmlFor="allowed-origins"
            >
              Allowed Origins (CORS)
            </label>
            <p className="settings-modal__hint">
              Browsers only. Add the origin of any page that calls the API from a different host or
              port, such as a reverse proxy. Clients still need a token. One per line.
            </p>
            <textarea
              id="allowed-origins"
              className="settings-modal__textarea"
              value={allowedOriginsText}
              onChange={(e) => {
                setAllowedOriginsText(e.target.value);
                setError("");
              }}
              placeholder="https://cctrace.example.com"
              spellCheck={false}
              rows={3}
            />

            <label className="settings-modal__label settings-modal__label--section">
              Accepted clients
            </label>
            {authSource === "disabled" ? (
              <p className="settings-modal__hint">
                Client verification is off (CCTRACE_API_AUTH=off): any local process can call the
                HTTP API. Unset the variable to require a registered client again.
              </p>
            ) : (
              <>
                <p className="settings-modal__hint">
                  Every client needs its own signed token (a JWT) to call the local HTTP API. Send
                  it as an <code>Authorization: Bearer</code> or <code>X-CCTrace-Token</code>{" "}
                  header. The bundled web UI and TUI register themselves; add a client below for any
                  script or tool of your own.
                </p>
                {authSource === "ephemeral" && (
                  <p className="settings-modal__hint settings-modal__hint--missing">
                    The signing key could not be written at startup, so credentials issued now stop
                    working when the app restarts and the TUI cannot read its own (see the server
                    log). Fix the config directory and restart.
                  </p>
                )}
                <table className="settings-modal__clients" aria-label="Accepted clients">
                  <thead>
                    <tr>
                      <th>Client</th>
                      <th>Issued</th>
                      <th>Status</th>
                      <th aria-label="Actions" />
                    </tr>
                  </thead>
                  <tbody>
                    {clients.map((client) => {
                      const revoked = client.revoked_at != null;
                      const busy = busyClient === client.id;
                      const armed = pending?.id === client.id ? pending.kind : null;
                      return (
                        <tr key={client.id} data-client={client.name}>
                          <td>
                            <span className="settings-modal__client-name">{client.name}</span>
                            {client.builtin && (
                              <span className="settings-modal__client-badge">built-in</span>
                            )}
                          </td>
                          <td>{formatDate(client.created_at)}</td>
                          <td
                            className={
                              revoked
                                ? "settings-modal__client-status settings-modal__client-status--revoked"
                                : "settings-modal__client-status"
                            }
                          >
                            {revoked ? "Revoked" : "Active"}
                          </td>
                          <td className="settings-modal__client-actions">
                            <button
                              type="button"
                              className="settings-modal__btn"
                              onClick={() => void handleReissue(client)}
                              disabled={busy}
                              aria-label={`Reissue ${client.name}`}
                              aria-pressed={armed === "reissue"}
                            >
                              {armed === "reissue" ? "Confirm reissue?" : "Reissue"}
                            </button>
                            <button
                              type="button"
                              className="settings-modal__btn"
                              onClick={() => void handleRevoke(client)}
                              disabled={busy || revoked}
                              aria-label={`Revoke ${client.name}`}
                              aria-pressed={armed === "revoke"}
                            >
                              {armed === "revoke" ? "Confirm revoke?" : "Revoke"}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
                <div className="settings-modal__credential-row">
                  <input
                    className="settings-modal__input"
                    type="text"
                    value={newClientName}
                    onChange={(e) => {
                      setNewClientName(e.target.value);
                      setError("");
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void handleRegister();
                      }
                    }}
                    placeholder="New client name (e.g. ci-script)"
                    aria-label="New client name"
                    spellCheck={false}
                    maxLength={64}
                  />
                  <button
                    type="button"
                    className="settings-modal__btn"
                    onClick={() => void handleRegister()}
                    disabled={busyClient === "new" || !newClientName.trim()}
                  >
                    Add client
                  </button>
                </div>
                {issued && (
                  <>
                    <p className="settings-modal__hint">
                      Credential for <strong>{issued.client.name}</strong> — store it now, it is not
                      kept:
                    </p>
                    <div className="settings-modal__credential-row">
                      <input
                        className="settings-modal__input settings-modal__input--credential"
                        type="text"
                        value={issued.credential}
                        readOnly
                        aria-label="New client credential"
                        spellCheck={false}
                        onFocus={(e) => e.currentTarget.select()}
                      />
                      <button
                        type="button"
                        className="settings-modal__btn"
                        onClick={handleCopyCredential}
                      >
                        Copy
                      </button>
                      <button
                        type="button"
                        className="settings-modal__btn"
                        onClick={() => setIssued(null)}
                        aria-label="Dismiss credential"
                      >
                        Done
                      </button>
                    </div>
                  </>
                )}
                {clientNotice && (
                  <p className="settings-modal__hint settings-modal__hint--effective">
                    {clientNotice}
                  </p>
                )}
              </>
            )}
          </>
        )}

        {(activeTab === "general" || activeTab === "appearance") && (
          <>
            <label className="settings-modal__label settings-modal__label--section">
              Font Size
            </label>
            <p className="settings-modal__hint">Zoom the whole interface in or out.</p>
            <div className="settings-modal__font-scale" role="group" aria-label="Font size">
              {FONT_SCALE_PRESETS.map((preset) => (
                <button
                  key={preset}
                  type="button"
                  className={
                    preset === fontScale
                      ? "settings-modal__font-scale-btn settings-modal__font-scale-btn--active"
                      : "settings-modal__font-scale-btn"
                  }
                  aria-pressed={preset === fontScale}
                  onClick={() => onFontScaleChange(preset)}
                >
                  {formatFontScale(preset)}
                </button>
              ))}
            </div>

            <label className="settings-modal__label settings-modal__label--section">
              Session Preview
            </label>
            <p className="settings-modal__hint">
              Show a session's end-of-session recap as its list preview, when the recap is the
              latest entry.
            </p>
            <button
              type="button"
              role="switch"
              aria-checked={recapPreview}
              aria-label="Recap preview"
              className={`settings-modal__toggle${recapPreview ? " settings-modal__toggle--on" : ""}`}
              onClick={() => onRecapPreviewChange(!recapPreview)}
            >
              <span className="settings-modal__toggle-knob" />
              <span className="settings-modal__toggle-label">{recapPreview ? "On" : "Off"}</span>
            </button>
          </>
        )}

        {activeTab === "analytics" && <AnalyticsSettings />}

        {activeTab !== "analytics" && error && <p className="settings-modal__error">{error}</p>}
        {activeTab !== "analytics" && (
          <div className="settings-modal__actions">
            <button
              className="settings-modal__btn settings-modal__btn--secondary"
              onClick={handleReset}
              disabled={saving}
            >
              Reset to Default
            </button>
            <button
              className="settings-modal__btn settings-modal__btn--primary"
              onClick={handleSave}
              disabled={saving}
            >
              Save
            </button>
          </div>
        )}
      </div>
    </PopoutModal>
  );
}
