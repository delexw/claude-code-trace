import { useCallback, useEffect, useState } from "react";
import { invoke } from "../lib/invoke";
import { isTauri } from "../lib/isTauri";
import type {
  AnalyticsSettings as AnalyticsSettingsValue,
  PayloadMode,
  RecommendationProvider,
} from "../types";
import { BetaBadge } from "./BetaBadge";
import { WarningIcon } from "./Icons";

const defaultSettings: AnalyticsSettingsValue = {
  jev: { configured: false, source: null, status: "not_configured" },
  defaultPayloadMode: "minimized",
  recommendationProvider: { type: "codex-subscription", model: null },
};

function statusLabel(status: string): string {
  return status.replaceAll("_", " ").replace(/^./, (first) => first.toUpperCase());
}

export function AnalyticsSettings() {
  const [settings, setSettings] = useState(defaultSettings);
  const [jevKey, setJevKey] = useState("");
  const [providerKey, setProviderKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    void invoke<AnalyticsSettingsValue>("get_analytics_settings")
      .then(setSettings)
      .catch((loadError) => setError(String(loadError)));
  }, []);

  const run = useCallback(async (label: string, action: () => Promise<void>) => {
    setBusy(label);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (actionError) {
      setError(String(actionError));
    } finally {
      setBusy(null);
    }
  }, []);

  const saveConfiguration = useCallback(
    async (next: AnalyticsSettingsValue) => {
      await run("settings", async () => {
        const saved = await invoke<AnalyticsSettingsValue>("set_analytics_settings", {
          defaultPayloadMode: next.defaultPayloadMode,
          recommendationProvider: next.recommendationProvider,
        });
        setSettings(saved);
        setNotice("Analytics settings saved.");
      });
    },
    [run],
  );

  const setProviderType = (type: RecommendationProvider["type"]) => {
    const recommendationProvider: RecommendationProvider =
      type === "openai-compatible"
        ? { type, baseUrl: "http://localhost:1234/v1", model: "", apiKeyConfigured: false }
        : { type, model: null };
    const next = { ...settings, recommendationProvider };
    setSettings(next);
    void saveConfiguration(next);
  };

  const provider = settings.recommendationProvider;
  const environmentKey = settings.jev.source === "environment";

  return (
    <div className="analytics-settings">
      <div className="analytics-settings__heading">
        <span>Analytics</span> <BetaBadge />
      </div>
      {!isTauri && (
        <div className="analytics-settings__security-notice" role="note">
          <WarningIcon />
          <div>
            <strong>API tokens cannot be entered in the browser</strong>
            <p>
              The web API does not accept token-saving or token-clearing requests. Set{" "}
              <code>JEV_API_KEY</code> before starting web mode, or use the desktop app to store
              provider tokens in your operating system credential store. The browser can use an
              existing server-side token but never receives it.
            </p>
          </div>
        </div>
      )}
      <section className="analytics-settings__section">
        <h3>Session Efficiency Analysis</h3>
        <p className="settings-modal__hint">
          Jev performs behavioural diagnosis. Session data is sent only after you trigger an
          analysis and confirm its privacy warning.
        </p>
        {isTauri && (
          <>
            <label className="settings-modal__label" htmlFor="jev-api-key">
              Jev API key
            </label>
            <div className="settings-modal__credential-row">
              <input
                id="jev-api-key"
                className="settings-modal__input"
                type="password"
                value={jevKey}
                disabled={environmentKey}
                placeholder={
                  environmentKey
                    ? "Configured via environment"
                    : settings.jev.configured
                      ? "Stored securely"
                      : "Enter Jev API key"
                }
                onChange={(event) => setJevKey(event.target.value)}
                autoComplete="off"
              />
              <button
                type="button"
                className="settings-modal__btn"
                disabled={environmentKey || !jevKey.trim() || busy !== null}
                onClick={() =>
                  void run("jev-key", async () => {
                    setSettings(
                      await invoke<AnalyticsSettingsValue>("set_jev_api_key", { key: jevKey }),
                    );
                    setJevKey("");
                    setNotice("Jev API key stored in the platform credential store.");
                  })
                }
              >
                {settings.jev.configured ? "Update" : "Save"}
              </button>
              <button
                type="button"
                className="settings-modal__btn"
                disabled={environmentKey || !settings.jev.configured || busy !== null}
                onClick={() =>
                  void run("jev-clear", async () => {
                    setSettings(await invoke<AnalyticsSettingsValue>("clear_jev_api_key"));
                    setNotice("Stored Jev API key cleared.");
                  })
                }
              >
                Clear
              </button>
            </div>
          </>
        )}
        <p className="settings-modal__hint">
          Status: {environmentKey ? "Configured via environment" : statusLabel(settings.jev.status)}
        </p>
        <button
          type="button"
          className="settings-modal__btn"
          disabled={!settings.jev.configured || busy !== null}
          onClick={() =>
            void run("jev-test", async () => {
              await invoke("test_jev_connection");
              setSettings((current) => ({
                ...current,
                jev: { ...current.jev, status: "connected" },
              }));
              setNotice("Jev connection successful.");
            })
          }
        >
          {busy === "jev-test" ? "Testing…" : "Test Jev connection"}
        </button>

        <label className="settings-modal__label settings-modal__label--section">
          Default payload mode
        </label>
        {(["minimized", "full-transcript"] as PayloadMode[]).map((mode) => (
          <label key={mode} className="analytics-settings__radio">
            <input
              type="radio"
              name="payload-mode"
              checked={settings.defaultPayloadMode === mode}
              onChange={() => {
                const next = { ...settings, defaultPayloadMode: mode };
                setSettings(next);
                void saveConfiguration(next);
              }}
            />
            {mode === "minimized" ? "Minimized data" : "Full transcript"}
          </label>
        ))}
      </section>

      <section className="analytics-settings__section">
        <h3>Improvement Recommendations</h3>
        <p className="settings-modal__hint">
          This provider is configured independently from Jev and is used for recommendation
          generation.
        </p>
        <label className="settings-modal__label" htmlFor="recommendation-provider">
          Provider
        </label>
        <select
          id="recommendation-provider"
          className="settings-modal__input"
          value={provider.type}
          onChange={(event) =>
            setProviderType(event.target.value as RecommendationProvider["type"])
          }
        >
          <option value="codex-subscription">Codex Subscription</option>
          <option value="claude-code-subscription">Claude Code Subscription</option>
          <option value="openai-compatible">OpenAI-Compatible Endpoint</option>
        </select>

        {provider.type === "openai-compatible" ? (
          <>
            <label className="settings-modal__label" htmlFor="provider-base-url">
              Base URL
            </label>
            <input
              id="provider-base-url"
              className="settings-modal__input"
              value={provider.baseUrl}
              onChange={(event) =>
                setSettings({
                  ...settings,
                  recommendationProvider: { ...provider, baseUrl: event.target.value },
                })
              }
            />
            <label className="settings-modal__label" htmlFor="provider-model">
              Model
            </label>
            <input
              id="provider-model"
              className="settings-modal__input"
              value={provider.model}
              onChange={(event) =>
                setSettings({
                  ...settings,
                  recommendationProvider: { ...provider, model: event.target.value },
                })
              }
            />
            {isTauri && (
              <>
                <label className="settings-modal__label" htmlFor="provider-api-key">
                  API key (optional)
                </label>
                <div className="settings-modal__credential-row">
                  <input
                    id="provider-api-key"
                    className="settings-modal__input"
                    type="password"
                    value={providerKey}
                    placeholder={provider.apiKeyConfigured ? "Stored securely" : "Optional"}
                    onChange={(event) => setProviderKey(event.target.value)}
                    autoComplete="off"
                  />
                  <button
                    type="button"
                    className="settings-modal__btn"
                    disabled={!providerKey.trim() || busy !== null}
                    onClick={() =>
                      void run("provider-key", async () => {
                        await invoke("set_recommendation_provider_api_key", { key: providerKey });
                        const next = { ...provider, apiKeyConfigured: true };
                        setProviderKey("");
                        setSettings({ ...settings, recommendationProvider: next });
                        await saveConfiguration({ ...settings, recommendationProvider: next });
                      })
                    }
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    className="settings-modal__btn"
                    disabled={!provider.apiKeyConfigured || busy !== null}
                    onClick={() =>
                      void run("provider-clear", async () => {
                        await invoke("clear_recommendation_provider_api_key");
                        const next = { ...provider, apiKeyConfigured: false };
                        setSettings({ ...settings, recommendationProvider: next });
                        await saveConfiguration({ ...settings, recommendationProvider: next });
                      })
                    }
                  >
                    Clear
                  </button>
                </div>
              </>
            )}
            <button
              type="button"
              className="settings-modal__btn"
              onClick={() => void saveConfiguration(settings)}
            >
              Save provider settings
            </button>
          </>
        ) : (
          <>
            <p className="settings-modal__hint">
              Authentication: Native{" "}
              {provider.type === "codex-subscription" ? "Codex" : "Claude Code"} subscription
            </p>
            <label className="settings-modal__label" htmlFor="provider-model">
              Model
            </label>
            <input
              id="provider-model"
              className="settings-modal__input"
              placeholder="default / auto"
              value={provider.model ?? ""}
              onChange={(event) =>
                setSettings({
                  ...settings,
                  recommendationProvider: { ...provider, model: event.target.value || null },
                })
              }
              onBlur={() => void saveConfiguration(settings)}
            />
          </>
        )}
        <button
          type="button"
          className="settings-modal__btn analytics-settings__test-provider"
          disabled={busy !== null}
          onClick={() =>
            void run("provider-test", async () => {
              await invoke("test_recommendation_provider", {
                provider: settings.recommendationProvider,
              });
              setNotice("Recommendation provider connected.");
            })
          }
        >
          {busy === "provider-test" ? "Testing…" : "Test recommendation provider"}
        </button>
      </section>
      {notice && <p className="settings-modal__hint settings-modal__hint--effective">{notice}</p>}
      {error && <p className="settings-modal__error">{error}</p>}
    </div>
  );
}
