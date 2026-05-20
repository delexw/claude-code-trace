import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/invoke";
import { PopoutModal } from "./PopoutModal";

interface SettingsResponse {
  projects_dir: string | null;
  default_dir: string;
}

interface SettingsModalProps {
  onClose: () => void;
  onSaved: () => void;
}

export function SettingsModal({ onClose, onSaved }: SettingsModalProps) {
  const [projectsDir, setProjectsDir] = useState("");
  const [defaultDir, setDefaultDir] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const load = async () => {
      try {
        const res = await invoke<SettingsResponse>("get_settings");
        setDefaultDir(res.default_dir);
        setProjectsDir(res.projects_dir ?? res.default_dir);
      } catch (err) {
        console.error("Failed to load settings:", err);
      }
    };
    void load();
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      await invoke<SettingsResponse>("set_projects_dir", { path: projectsDir.trim() || null });
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [projectsDir, onSaved, onClose]);

  const handleReset = useCallback(async () => {
    setSaving(true);
    setError("");
    try {
      const res = await invoke<SettingsResponse>("set_projects_dir", { path: null });
      setProjectsDir(res.default_dir);
      onSaved();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }, [onSaved, onClose]);

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
      initialHeight={240}
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
          placeholder={defaultDir}
          spellCheck={false}
          autoFocus
        />
        <p className="settings-modal__hint">Default: {defaultDir}</p>
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
