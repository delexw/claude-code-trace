// Theme constants — dark theme colors matching the Go TUI (theme.go)

export const colors = {
  // Background
  bg: "#1a1a2e",
  bgSurface: "#16213e",
  bgElevated: "#222244",
  bgHover: "#2a2a4a",

  // Text hierarchy
  textPrimary: "#d0d0d0", // 252
  textSecondary: "#8a8a8a", // 245
  textDim: "#767676", // 243
  textMuted: "#585858", // 240

  // Accents
  accent: "#5fafff", // 75 — blue
  error: "#ff0000", // 196 — red
  info: "#5f87ff", // 69 — blue

  // Surfaces
  border: "#5f5f87", // 60 — muted blue

  // Model family (matches claude-devtools)
  modelOpus: "#ff5f87", // 204 — coral
  modelSonnet: "#5fafff", // 75 — blue
  modelHaiku: "#87d787", // 114 — green

  // Token highlight
  tokenHigh: "#ff8700", // 208 — orange (>150k)

  // Ongoing indicator
  ongoing: "#5faf00", // 76 — green

  // Context usage thresholds
  contextOk: "#87d787", // 114 — green <50%
  contextWarn: "#ff8700", // 208 — yellow/orange 50-80%
  contextCrit: "#ff0000", // 196 — red >80%

  // Permission mode pill backgrounds
  pillBypass: "#ff0000", // 196 — red: bypassPermissions
  pillAcceptEdits: "#af5fff", // 135 — purple: acceptEdits
  pillPlan: "#87d787", // 114 — green: plan

  // Picker
  pickerSelectedBg: "#3a3a3a", // 237

  // Git branch
  gitBranch: "#af5fff", // 135 — purple

  // Tool category colors (all dim for now)
  toolRead: "#767676",
  toolEdit: "#767676",
  toolWrite: "#767676",
  toolBash: "#767676",
  toolGrep: "#767676",
  toolGlob: "#767676",
  toolTask: "#767676",
  toolSkill: "#767676",
  toolWeb: "#767676",
  toolOther: "#767676",
} as const;

// Team member colors (matches claude-devtools teamColors.ts)
export const teamColors: Record<string, string> = {
  blue: "#5fafff", // 75
  green: "#87d787", // 114
  red: "#ff5f87", // 204
  yellow: "#ffdf00", // 220
  purple: "#d787ff", // 177
  cyan: "#5fafaf", // 80
  orange: "#ff8700", // 208
  pink: "#ff87af", // 211
};

export function getModelColor(model: string): string {
  if (model.includes("opus")) return colors.modelOpus;
  if (model.includes("sonnet")) return colors.modelSonnet;
  if (model.includes("haiku")) return colors.modelHaiku;
  return colors.textSecondary;
}

export function getTeamColor(name: string): string {
  return teamColors[name.toLowerCase()] ?? colors.accent;
}

export function getContextColor(pct: number): string {
  if (pct < 50) return colors.contextOk;
  if (pct < 80) return colors.contextWarn;
  return colors.contextCrit;
}

export function getPillColor(mode: string): string {
  switch (mode) {
    case "bypassPermissions":
      return colors.pillBypass;
    case "acceptEdits":
      return colors.pillAcceptEdits;
    case "plan":
      return colors.pillPlan;
    default:
      return colors.textMuted;
  }
}

// Tool category icon labels (unicode fallbacks for web)
export const toolCategoryIcons: Record<string, string> = {
  Read: "\u{1F4D6}",
  Edit: "\u{270F}\u{FE0F}",
  Write: "\u{270F}\u{FE0F}",
  Bash: "\u{1F527}",
  Grep: "\u{1F50D}",
  Glob: "\u{1F50D}",
  Task: "\u{1F916}",
  Tool: "\u{1F527}",
  Web: "\u{1F310}",
  Other: "\u{1F527}",
};

// Task status icons
export const taskStatusIcons: Record<string, string> = {
  completed: "\u2713",
  in_progress: "\u27F3",
  pending: "\u25CB",
};

// Spinner frames (braille)
export const spinnerFrames = [
  "\u280B",
  "\u2819",
  "\u2839",
  "\u2838",
  "\u283C",
  "\u2834",
  "\u2826",
  "\u2827",
  "\u2807",
  "\u280F",
];
