import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "../lib/invoke";
import type { ApiAuthSource, ApiClient, SettingsResponse } from "../types";
import type { SettingsPanelHandle, SettingsTab } from "../plugins/types";
import { PLUGINS, isPluginEnabled } from "../plugins/registry";
import { setApiToken as setLiveApiToken } from "../lib/apiToken";
import { PopoutModal } from "./PopoutModal";
import { FONT_SCALE_PRESETS, formatFontScale } from "../lib/fontScale";
import { AnalyticsSettings } from "./AnalyticsSettings";
import { BetaBadge } from "./BetaBadge";

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
  /** A plugin was turned on or off; `settings` is the backend's answer. */
  onPluginsChanged?: (settings: SettingsResponse) => void;
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

/** The mounted plugin panels' handles, in registry order. */
function mountedPanelHandles(handles: Map<string, SettingsPanelHandle>): SettingsPanelHandle[] {
  return PLUGINS.flatMap((plugin) => {
    const handle = handles.get(plugin.id);
    return handle ? [handle] : [];
  });
}

/** The last panel's settings answer — each one is the whole, freshly saved settings. */
function lastResponse(responses: (SettingsResponse | undefined)[]): SettingsResponse | undefined {
  return responses.findLast((res) => res !== undefined);
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
  onPluginsChanged,
  fontScale,
  onFontScaleChange,
  recapPreview,
  onRecapPreviewChange,
  initialTab = "general",
}: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
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
    setSettings(res);
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
    };
    void load();
  }, [applyResponse]);

  const panels = PLUGINS.filter(
    (plugin) => plugin.SettingsPanel && isPluginEnabled(plugin, settings?.plugins),
  );
  const panelHandles = useRef(new Map<string, SettingsPanelHandle>());
  const panelRegistrations = useMemo(
    () =>
      Object.fromEntries(
        PLUGINS.map((plugin) => [
          plugin.id,
          (handle: SettingsPanelHandle | null) => {
            if (handle) panelHandles.current.set(plugin.id, handle);
            else panelHandles.current.delete(plugin.id);
          },
        ]),
      ),
    [panelHandles],
  );
  const clearError = useCallback(() => setError(""), []);
  const [togglingPlugin, setTogglingPlugin] = useState<string | null>(null);
  const togglePlugin = useCallback(
    async (id: string, enabled: boolean) => {
      setTogglingPlugin(id);
      setError("");
      try {
        const res = await invoke<SettingsResponse>("set_plugin_enabled", { id, enabled });
        applyResponse(res);
        onPluginsChanged?.(res);
      } catch (err) {
        setError(String(err));
      } finally {
        setTogglingPlugin(null);
      }
    },
    [applyResponse, onPluginsChanged],
  );
  const optionalPlugins = (settings?.plugins ?? []).filter((plugin) => plugin.can_disable);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const panelRes = lastResponse(
        await Promise.all(mountedPanelHandles(panelHandles.current).map((h) => h.save())),
      );
      const originsRes = await invoke<SettingsResponse>("set_allowed_origins", {
        origins: parseOrigins(allowedOriginsText),
      });
      const res = originsRes ?? panelRes;
      if (res) applyResponse(res);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [panelHandles, allowedOriginsText, applyResponse, onSaved, onClose]);

  const handleReset = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const res = lastResponse(
        await Promise.all(mountedPanelHandles(panelHandles.current).map((h) => h.reset?.())),
      );
      if (res) applyResponse(res);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [panelHandles, applyResponse, onSaved, onClose]);

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

  const handleSaveRef = useRef(handleSave);
  useEffect(() => {
    handleSaveRef.current = handleSave;
  });
  const submit = useCallback(() => void handleSaveRef.current(), []);

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
        {activeTab === "general" && optionalPlugins.length > 0 && (
          <>
            <label className="settings-modal__label">Sources</label>
            <p className="settings-modal__hint">
              Where sessions come from besides Claude Code. A source that is off is hidden
              everywhere.
            </p>
            {optionalPlugins.map((plugin) => (
              <div key={plugin.id} className="settings-modal__source">
                <span className="settings-modal__source-name">{plugin.label}</span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={plugin.enabled}
                  aria-label={`Show ${plugin.label} sessions`}
                  className={`settings-modal__toggle${plugin.enabled ? " settings-modal__toggle--on" : ""}`}
                  onClick={() => void togglePlugin(plugin.id, !plugin.enabled)}
                  disabled={togglingPlugin === plugin.id}
                >
                  <span className="settings-modal__toggle-knob" />
                  <span className="settings-modal__toggle-label">
                    {plugin.enabled ? "On" : "Off"}
                  </span>
                </button>
              </div>
            ))}
          </>
        )}

        {/* Kept mounted on every tab so unsaved edits survive a tab switch and Save
            from any tab still saves them, as the rest of this modal's fields do. */}
        <div style={{ display: activeTab === "general" ? "contents" : "none" }}>
          {panels.map(({ id, SettingsPanel }) =>
            SettingsPanel ? (
              <SettingsPanel
                key={id}
                settings={settings}
                onEdited={clearError}
                onSubmit={submit}
                register={panelRegistrations[id]}
              />
            ) : null,
          )}
        </div>

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
