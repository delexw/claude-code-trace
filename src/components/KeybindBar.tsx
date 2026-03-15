import type { ViewState } from "../types";

interface KeybindBarProps {
  view: ViewState;
  hasTeams: boolean;
  showHints?: boolean;
  onToggle?: () => void;
  actions?: Record<string, () => void>;
}

interface KeyHint {
  key: string;
  label: string;
}

const listKeys: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "\u2191/\u2193", label: "scroll" },
  { key: "G/g", label: "jump" },
  { key: "Tab", label: "toggle" },
  { key: "Enter", label: "detail" },
  { key: "d", label: "debug" },
  { key: "e/c", label: "expand/collapse" },
  { key: "q", label: "sessions" },
];

const listKeysWithTeams: KeyHint[] = [
  ...listKeys.slice(0, 6),
  { key: "t", label: "tasks" },
  ...listKeys.slice(6),
];

const detailKeys: KeyHint[] = [
  { key: "j/k", label: "items" },
  { key: "Tab", label: "toggle" },
  { key: "Enter", label: "open" },
  { key: "h/l", label: "panels" },
  { key: "q/Esc", label: "back" },
];

const pickerKeys: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "Enter", label: "open" },
  { key: "/", label: "search" },
  { key: "q/Esc", label: "back" },
];

const debugKeys: KeyHint[] = [{ key: "q/Esc", label: "back" }];

const teamKeys: KeyHint[] = [{ key: "q/Esc", label: "back" }];

function getKeys(view: ViewState, hasTeams: boolean): KeyHint[] {
  switch (view) {
    case "list":
      return hasTeams ? listKeysWithTeams : listKeys;
    case "detail":
      return detailKeys;
    case "picker":
      return pickerKeys;
    case "debug":
      return debugKeys;
    case "team":
      return teamKeys;
  }
}

export function KeybindBar({
  view,
  hasTeams,
  showHints = true,
  onToggle,
  actions,
}: KeybindBarProps) {
  const keys = getKeys(view, hasTeams);

  return (
    <div className="keybind-bar">
      {showHints &&
        keys.map((hint) => {
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
  );
}
