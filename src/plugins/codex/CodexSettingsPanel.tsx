import { useCallback, useEffect, useState } from "react";
import { invoke } from "../../lib/invoke";
import type { SettingsPanelProps } from "../types";
import type { CodexSettings } from "./types";

export function CodexSettingsPanel({ onEdited, onSubmit, register }: SettingsPanelProps) {
  const [loaded, setLoaded] = useState<CodexSettings | null>(null);
  const [rootInput, setRootInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const apply = useCallback((settings: CodexSettings) => {
    setLoaded(settings);
    setRootInput(settings.root ?? "");
    setError(null);
  }, []);

  useEffect(() => {
    let stopped = false;
    invoke<CodexSettings>("get_codex_settings")
      .then((settings) => {
        if (!stopped) apply(settings);
      })
      .catch((failure) => {
        if (!stopped)
          setError(
            `Could not load Codex settings: ${failure instanceof Error ? failure.message : String(failure)}`,
          );
      });
    return () => {
      stopped = true;
    };
  }, [apply]);

  const save = useCallback(async () => {
    apply(await invoke<CodexSettings>("set_codex_root", { path: rootInput.trim() || null }));
    return undefined;
  }, [apply, rootInput]);
  const reset = useCallback(async () => {
    apply(await invoke<CodexSettings>("set_codex_root", { path: null }));
    return undefined;
  }, [apply]);
  useEffect(() => {
    register({ save, reset });
    return () => register(null);
  }, [register, save, reset]);

  return (
    <>
      <label className="settings-modal__label settings-modal__label--section" htmlFor="codex-root">
        Codex Home Directory
      </label>
      <p className="settings-modal__hint">
        Leave empty to find Codex chats in CODEX_HOME or ~/.codex and available WSL homes. Set a
        directory containing sessions and archived_sessions to read a specific Codex home.
      </p>
      <input
        id="codex-root"
        className="settings-modal__input"
        type="text"
        value={rootInput}
        placeholder={loaded?.roots[0]?.path ?? ""}
        spellCheck={false}
        onChange={(event) => {
          setRootInput(event.target.value);
          setError(null);
          onEdited();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            onSubmit();
          }
        }}
      />
      {error && (
        <p className="settings-modal__hint settings-modal__hint--missing" role="alert">
          {error}
        </p>
      )}
      {loaded?.roots.map((root) => (
        <p
          key={`${root.source}:${root.path}`}
          className={`settings-modal__hint ${root.exists ? "settings-modal__hint--effective" : "settings-modal__hint--missing"}`}
        >
          {root.exists ? "Reading from:" : "Not found:"} {root.path} ({root.source})
        </p>
      ))}
    </>
  );
}
