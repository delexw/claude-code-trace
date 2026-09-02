import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import { isTauri } from "../lib/isTauri";
import { setApiToken as setLiveApiToken } from "../lib/apiToken";
import { reconnectSse } from "../lib/listen";
import { PopoutModal } from "./PopoutModal";
import { FONT_SCALE_PRESETS, formatFontScale } from "../lib/fontScale";

interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
  effective_dir: string;
  effective_dir_exists: boolean;
  wsl_distros: string[];
  allowed_origins: string[];
  /** Whether the HTTP API requires the shared client token. */
  api_auth_enabled?: boolean;
  /** "file" (rotatable here), "env" (CCTRACE_API_TOKEN, read-only) or "disabled". */
  api_token_source?: ApiTokenSource;
  /** The token accepted clients must present; null when disabled. */
  api_token?: string | null;
}

type ApiTokenSource = "file" | "env" | "disabled";

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
}

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
}: SettingsModalProps) {
  const [projectsDir, setProjectsDir] = useState("");
  const [defaultDir, setDefaultDir] = useState("");
  const [effectiveDir, setEffectiveDir] = useState("");
  const [effectiveDirExists, setEffectiveDirExists] = useState(true);
  const [availableDistros, setAvailableDistros] = useState<string[]>([]);
  const [selectedDistros, setSelectedDistros] = useState<Set<string>>(new Set());
  const [allowedOriginsText, setAllowedOriginsText] = useState("");
  const [apiToken, setApiTokenState] = useState<string | null>(null);
  const [apiTokenSource, setApiTokenSource] = useState<ApiTokenSource>("file");
  const [showToken, setShowToken] = useState(false);
  const [confirmRegen, setConfirmRegen] = useState(false);
  const [regenerating, setRegenerating] = useState(false);
  const [tokenNotice, setTokenNotice] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const applyResponse = useCallback((res: SettingsResponse) => {
    setDefaultDir(res.default_dir);
    setProjectsDir(res.projects_dir ?? "");
    setEffectiveDir(res.effective_dir);
    setEffectiveDirExists(res.effective_dir_exists);
    setSelectedDistros(new Set(res.wsl_distros ?? []));
    setAllowedOriginsText((res.allowed_origins ?? []).join("\n"));
    setApiTokenState(res.api_token ?? null);
    setApiTokenSource(
      res.api_token_source ?? (res.api_auth_enabled === false ? "disabled" : "file"),
    );
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

  const handleCopyToken = useCallback(async () => {
    if (!apiToken) return;
    try {
      await navigator.clipboard.writeText(apiToken);
      setTokenNotice("Token copied to clipboard.");
    } catch {
      setTokenNotice("Could not access the clipboard — use Show and copy it manually.");
    }
  }, [apiToken]);

  // Two-click confirm: the first click arms the button, the second rotates.
  // Rotation invalidates every other client's token, so it shouldn't be one
  // accidental click away.
  const handleRegenerate = useCallback(async () => {
    if (!confirmRegen) {
      setConfirmRegen(true);
      setTokenNotice(
        "Click again to confirm. Other clients (TUI, scripts) will need the new token.",
      );
      return;
    }
    setConfirmRegen(false);
    setRegenerating(true);
    setError("");
    try {
      const res = await invoke<SettingsResponse>("regenerate_api_token");
      applyResponse(res);
      // Keep this tab working: send the new token from now on and reopen the
      // SSE stream, which was authenticated with the old one.
      setLiveApiToken(res.api_token);
      if (!isTauri) reconnectSse();
      setTokenNotice("Token regenerated — update the TUI and any scripts that used the old one.");
    } catch (err) {
      setError(String(err));
    } finally {
      setRegenerating(false);
    }
  }, [confirmRegen, applyResponse]);

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
      initialHeight={460}
    >
      <div className="settings-modal">
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

        <label className="settings-modal__label settings-modal__label--section">WSL Distros</label>
        {distros.length === 0 ? (
          <p className="settings-modal__hint">
            No WSL distributions detected. Sessions created inside WSL appear here once a distro is
            installed.
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

        <label
          className="settings-modal__label settings-modal__label--section"
          htmlFor="allowed-origins"
        >
          Allowed Origins (CORS)
        </label>
        <p className="settings-modal__hint">
          Extra origins allowed to call the local API (e.g. a reverse proxy or custom hostname), one
          per line.
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

        <label className="settings-modal__label settings-modal__label--section">API access</label>
        {apiTokenSource === "disabled" ? (
          <p className="settings-modal__hint">
            Client verification is off (CCTRACE_API_AUTH=off): any local process can call the HTTP
            API. Unset the variable to require the token again.
          </p>
        ) : (
          <>
            <p className="settings-modal__hint">
              Only clients presenting this token can call the local HTTP API. The bundled web UI and
              TUI pick it up automatically; give it to other tools as an{" "}
              <code>X-CCTrace-Token</code> header.
            </p>
            <div className="settings-modal__token-row">
              <input
                className="settings-modal__input settings-modal__input--token"
                type={showToken ? "text" : "password"}
                value={apiToken ?? ""}
                readOnly
                aria-label="API token"
                spellCheck={false}
              />
              <button
                type="button"
                className="settings-modal__btn"
                onClick={() => setShowToken((v) => !v)}
              >
                {showToken ? "Hide" : "Show"}
              </button>
              <button
                type="button"
                className="settings-modal__btn"
                onClick={handleCopyToken}
                disabled={!apiToken}
              >
                Copy
              </button>
              <button
                type="button"
                className="settings-modal__btn"
                onClick={handleRegenerate}
                disabled={regenerating || apiTokenSource === "env"}
                title={apiTokenSource === "env" ? "Set by CCTRACE_API_TOKEN" : undefined}
              >
                {confirmRegen ? "Confirm regenerate?" : "Regenerate"}
              </button>
            </div>
            {apiTokenSource === "env" && (
              <p className="settings-modal__hint">
                The token is set by CCTRACE_API_TOKEN, so it cannot be regenerated here.
              </p>
            )}
            {tokenNotice && (
              <p className="settings-modal__hint settings-modal__hint--effective">{tokenNotice}</p>
            )}
          </>
        )}

        <label className="settings-modal__label settings-modal__label--section">Font Size</label>
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
          Show a session's end-of-session recap as its list preview, when the recap is the latest
          entry.
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

        {error && <p className="settings-modal__error">{error}</p>}
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
      </div>
    </PopoutModal>
  );
}
