import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import { PopoutModal } from "./PopoutModal";

interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
  effective_dir: string;
  effective_dir_exists: boolean;
  wsl_distros: string[];
}

interface SettingsModalProps {
  onClose: () => void;
  onSaved: () => void;
}

/** Merge detected distros with already-configured ones so configured-but-offline
 * distros still appear (and stay toggleable) even when WSL isn't reporting them. */
function mergeDistros(available: string[], configured: string[]): string[] {
  const seen = new Set(available);
  return [...available, ...configured.filter((d) => !seen.has(d))];
}

export function SettingsModal({ onClose, onSaved }: SettingsModalProps) {
  const [projectsDir, setProjectsDir] = useState("");
  const [defaultDir, setDefaultDir] = useState("");
  const [effectiveDir, setEffectiveDir] = useState("");
  const [effectiveDirExists, setEffectiveDirExists] = useState(true);
  const [availableDistros, setAvailableDistros] = useState<string[]>([]);
  const [selectedDistros, setSelectedDistros] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const applyResponse = useCallback((res: SettingsResponse) => {
    setDefaultDir(res.default_dir);
    setProjectsDir(res.projects_dir ?? "");
    setEffectiveDir(res.effective_dir);
    setEffectiveDirExists(res.effective_dir_exists);
    setSelectedDistros(new Set(res.wsl_distros ?? []));
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
      applyResponse(wslRes ?? dirRes);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [projectsDir, selectedDistros, applyResponse, onSaved, onClose]);

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
      initialHeight={400}
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
