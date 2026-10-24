import { useCallback, useEffect, useState } from "react";
import { invoke } from "../../lib/invoke";
import type { SettingsPanelProps } from "../types";
import type { AntigravitySettings } from "./types";

export function AntigravitySettingsPanel({ onEdited, onSubmit, register }: SettingsPanelProps) {
  const [loaded, setLoaded] = useState<AntigravitySettings | null>(null);
  const [rootInput, setRootInput] = useState("");

  const apply = useCallback((settings: AntigravitySettings) => {
    setLoaded(settings);
    setRootInput(settings.root ?? "");
  }, []);

  useEffect(() => {
    invoke<AntigravitySettings>("get_antigravity_settings")
      .then(apply)
      .catch((err) => console.error("Failed to load Antigravity settings:", err));
  }, [apply]);

  const save = useCallback(async () => {
    apply(
      await invoke<AntigravitySettings>("set_antigravity_root", {
        path: rootInput.trim() || null,
      }),
    );
    return undefined;
  }, [apply, rootInput]);

  const reset = useCallback(async () => {
    apply(await invoke<AntigravitySettings>("set_antigravity_root", { path: null }));
    return undefined;
  }, [apply]);

  useEffect(() => {
    register({ save, reset });
    return () => register(null);
  }, [register, save, reset]);

  const active = loaded?.roots[0];
  const found = loaded?.roots.filter((root) => root.exists) ?? [];

  return (
    <>
      <label
        className="settings-modal__label settings-modal__label--section"
        htmlFor="antigravity-root"
      >
        Antigravity Brain Directory
      </label>
      <p className="settings-modal__hint">
        Read-only: conversations open in a viewer and the files are never modified. Leave empty to
        auto-detect every Antigravity version installed for this user, or set a path to read one
        specific directory instead.
      </p>
      <input
        id="antigravity-root"
        className="settings-modal__input"
        type="text"
        value={rootInput}
        onChange={(e) => {
          setRootInput(e.target.value);
          onEdited();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSubmit();
          }
        }}
        placeholder={active ? `${active.path} (auto-detected)` : ""}
        spellCheck={false}
      />
      {active && (
        <p
          className={
            active.exists
              ? "settings-modal__hint settings-modal__hint--effective"
              : "settings-modal__hint settings-modal__hint--missing"
          }
        >
          {active.exists ? "✓ Reading from:" : "✗ Not found:"} {active.path}
        </p>
      )}
      {found.length > 1 && (
        <p className="settings-modal__hint">
          Also found: {found.map((root) => `${root.flavor} → ${root.path}`).join(" · ")}
        </p>
      )}
    </>
  );
}
