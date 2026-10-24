import type { ReactNode } from "react";
import type { KeyHint } from "../plugins/types";

interface KeybindBarProps {
  /** Plugin status shown at the left (e.g. Claude's indexing progress). */
  status?: ReactNode;
  hints: KeyHint[];
  showHints?: boolean;
  onToggle?: () => void;
  /** Clickable hints, keyed by hint label. */
  actions?: Record<string, () => void>;
}

export const PICKER_HINTS: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "Enter", label: "open" },
  { key: "/", label: "search" },
  { key: "q/Esc", label: "back" },
];

export function KeybindBar({
  status,
  hints,
  showHints = true,
  onToggle,
  actions,
}: KeybindBarProps) {
  return (
    <div className="keybind-bar">
      {status}
      <div className="keybind-bar__hints">
        {showHints &&
          hints.map((hint) => {
            const action = actions?.[hint.label];
            return (
              <span
                key={hint.key}
                className={`keybind-bar__item${action ? " keybind-bar__item--clickable" : ""}`}
                onClick={action}
              >
                <span className="keybind-bar__key">{hint.key}</span>
                <span className="keybind-bar__label">{hint.label}</span>
              </span>
            );
          })}
        {onToggle && (
          <button
            className="keybind-bar__toggle"
            onClick={onToggle}
            title={showHints ? "Hide keybinds" : "Show keybinds"}
          >
            ?
          </button>
        )}
      </div>
    </div>
  );
}
