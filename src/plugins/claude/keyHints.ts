import type { KeyHint } from "../types";

const listKeys: KeyHint[] = [
  { key: "j/k", label: "nav" },
  { key: "↑/↓", label: "scroll" },
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

const backKeys: KeyHint[] = [{ key: "q/Esc", label: "back" }];

export type ClaudeView = "list" | "detail" | "team" | "debug";

export function claudeKeyHints(view: ClaudeView, hasTeams: boolean): KeyHint[] {
  switch (view) {
    case "list":
      return hasTeams ? listKeysWithTeams : listKeys;
    case "detail":
      return detailKeys;
    case "debug":
    case "team":
      return backKeys;
  }
}
