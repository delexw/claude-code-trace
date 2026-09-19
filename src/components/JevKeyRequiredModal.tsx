import { PopoutModal } from "./PopoutModal";

interface JevKeyRequiredModalProps {
  onCancel: () => void;
  onOpenSettings: () => void;
}

export function JevKeyRequiredModal({ onCancel, onOpenSettings }: JevKeyRequiredModalProps) {
  return (
    <PopoutModal
      onClose={onCancel}
      header={<span className="settings-modal__title">Jev API key required</span>}
      initialWidth={460}
      initialHeight={250}
    >
      <div className="efficiency-privacy">
        <p>
          Configure your Jev API key in Settings → Analytics before running external session
          analysis.
        </p>
        <div className="settings-modal__actions">
          <button type="button" className="settings-modal__btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="settings-modal__btn settings-modal__btn--primary"
            onClick={onOpenSettings}
          >
            Open Analytics Settings
          </button>
        </div>
      </div>
    </PopoutModal>
  );
}
