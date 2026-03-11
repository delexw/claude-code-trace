import type { ViewState } from "../types";

interface KeybindBarProps {
  view: ViewState;
  hasTeams: boolean;
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
  { key: "\u2191/\u2193", label: "scroll" },
  { key: "J/K", label: "page" },
  { key: "G/g", label: "jump" },
  { key: "q/Esc", label: "back" },
];

const pickerKeys: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "Enter", label: "open" },
  { key: "/", label: "search" },
  { key: "q/Esc", label: "back" },
];

const debugKeys: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "f", label: "level filter" },
  { key: "/", label: "search" },
  { key: "Tab", label: "expand" },
  { key: "q/Esc", label: "back" },
];

const teamKeys: KeyHint[] = [
  { key: "j/k", label: "scroll" },
  { key: "G/g", label: "jump" },
  { key: "q/Esc", label: "back" },
];

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

export function KeybindBar({ view, hasTeams }: KeybindBarProps) {
  const keys = getKeys(view, hasTeams);

  return (
    <div className="keybind-bar">
      {keys.map((hint) => (
        <span key={hint.key} className="keybind-bar__item">
          <span className="keybind-bar__key">{hint.key}</span>
          <span className="keybind-bar__label">{hint.label}</span>
        </span>
      ))}
    </div>
  );
}
