import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import { PopoutModal } from "./PopoutModal";

interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
  effective_dir: string;
  effective_dir_exists: boolean;
}

interface SettingsModalProps {
  onClose: () => void;
  onSaved: () => void;
}

export function SettingsModal({ onClose, onSaved }: SettingsModalProps) {
  const [projectsDir, setProjectsDir] = useState("");
  const [defaultDir, setDefaultDir] = useState("");
  const [effectiveDir, setEffectiveDir] = useState("");
  const [effectiveDirExists, setEffectiveDirExists] = useState(true);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const applyResponse = useCallback((res: SettingsResponse) => {
    setDefaultDir(res.default_dir);
    setProjectsDir(res.projects_dir ?? "");
    setEffectiveDir(res.effective_dir);
    setEffectiveDirExists(res.effective_dir_exists);
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

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const res = await invoke<SettingsResponse>("set_projects_dir", {
        path: projectsDir.trim() || null,
      });
      applyResponse(res);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [projectsDir, applyResponse, onSaved, onClose]);

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

  return (
    <PopoutModal
      onClose={onClose}
      header={<span className="settings-modal__title">Settings</span>}
      initialWidth={520}
      initialHeight={260}
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
