import { useCallback, useEffect, useState } from "react";
import { invoke } from "../../lib/invoke";
import type { SettingsResponse } from "../../types";
import type { SettingsPanelProps } from "../types";

/** Merge detected distros with already-configured ones so configured-but-offline
 * distros still appear (and stay toggleable) even when WSL isn't reporting them. */
function mergeDistros(available: string[], configured: string[]): string[] {
  const seen = new Set(available);
  return [...available, ...configured.filter((d) => !seen.has(d))];
}

export function ClaudeSettingsPanel({
  settings,
  onEdited,
  onSubmit,
  register,
}: SettingsPanelProps) {
  const [projectsDir, setProjectsDir] = useState(settings?.projects_dir ?? "");
  const [availableDistros, setAvailableDistros] = useState<string[]>([]);
  const [selectedDistros, setSelectedDistros] = useState(
    () => new Set(settings?.wsl_distros ?? []),
  );

  // Fresh settings (first load, or the answer to a save) replace the editable copies.
  const [loadedSettings, setLoadedSettings] = useState(settings);
  if (settings !== loadedSettings) {
    setLoadedSettings(settings);
    if (settings) {
      setProjectsDir(settings.projects_dir ?? "");
      setSelectedDistros(new Set(settings.wsl_distros ?? []));
    }
  }

  useEffect(() => {
    invoke<string[]>("list_wsl_distros")
      .then((distros) => setAvailableDistros(distros ?? []))
      .catch((err) => console.error("Failed to list WSL distros:", err));
  }, []);

  const save = useCallback(async () => {
    const dirRes = await invoke<SettingsResponse>("set_projects_dir", {
      path: projectsDir.trim() || null,
    });
    const wslRes = await invoke<SettingsResponse>("set_wsl_distros", {
      distros: [...selectedDistros],
    });
    return wslRes ?? dirRes;
  }, [projectsDir, selectedDistros]);

  const reset = useCallback(() => invoke<SettingsResponse>("set_projects_dir", { path: null }), []);

  useEffect(() => {
    register({ save, reset });
    return () => register(null);
  }, [register, save, reset]);

  const toggleDistro = useCallback((name: string) => {
    setSelectedDistros((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  const defaultDir = settings?.default_dir ?? "";
  const effectiveDir = settings?.effective_dir ?? "";
  const effectiveDirExists = settings?.effective_dir_exists ?? true;
  const distros = mergeDistros(availableDistros, [...selectedDistros]);

  return (
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
          onEdited();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onSubmit();
          }
        }}
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
    </>
  );
}
