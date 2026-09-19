import { useEffect, useRef } from "react";
import { VscCheck, VscError } from "react-icons/vsc";

interface OperationResultModalProps {
  kind: "success" | "error";
  message: string;
  onClose: () => void;
}

export function OperationResultModal({ kind, message, onClose }: OperationResultModalProps) {
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeButton.current?.focus();
  }, []);

  return (
    <div
      className="operation-result-overlay"
      onClick={(event) => {
        event.stopPropagation();
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className={`operation-result-modal operation-result-modal--${kind}`}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="operation-result-title"
        aria-describedby="operation-result-message"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="operation-result-modal__icon" aria-hidden="true">
          {kind === "success" ? <VscCheck /> : <VscError />}
        </div>
        <div className="operation-result-modal__content">
          <h3 id="operation-result-title">{kind === "success" ? "Success" : "Action failed"}</h3>
          <p id="operation-result-message">{message}</p>
        </div>
        <button
          ref={closeButton}
          type="button"
          className="settings-modal__btn settings-modal__btn--primary"
          onClick={onClose}
        >
          OK
        </button>
      </div>
    </div>
  );
}
