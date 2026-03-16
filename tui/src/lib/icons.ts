/**
 * Centralized icon/glyph definitions for the TUI.
 * Matches the Go TUI's icons.go — all Nerd Font glyphs use explicit
 * Unicode escapes to prevent corruption.
 *
 * Requires a Nerd Font patched terminal font for proper rendering.
 */

// ── Role Icons ──────────────────────────────────────────────────────

export const IconClaude = "\u{F167A}"; // nf-md-robot_outline (U+F167A)
export const IconUser = "\uF007"; // nf-fa-user (U+F007)
export const IconSystem = "\uF120"; // nf-fa-terminal (U+F120)
export const IconSystemErr = "\uF06A"; // nf-fa-circle_exclamation (U+F06A)

// ── Item Type Icons ─────────────────────────────────────────────────

export const IconThinking = "\uF0EB"; // nf-fa-lightbulb (U+F0EB)
export const IconOutput = "\u{F0182}"; // nf-md-comment_outline (U+F0182)
export const IconTool = "\u{F0BE0}"; // nf-md-wrench_outline (U+F0BE0)
export const IconSubagent = "\u{F167A}"; // nf-md-robot_outline (U+F167A)
export const IconTeammate = "\u{F167A}"; // nf-md-robot_outline (U+F167A)
export const IconHook = "\uF0E7"; // nf-fa-bolt (U+F0E7) — lightning for hooks/events

// ── Navigation/Cursor Icons ─────────────────────────────────────────

export const IconSelected = "\u2502"; // box drawing vertical │ (U+2502)
export const IconExpanded = "\uF078"; // nf-fa-chevron_down (U+F078)
export const IconCollapsed = "\uF054"; // nf-fa-chevron_right (U+F054)
export const IconDrillDown = "\uF061"; // nf-fa-arrow_right (U+F061)
export const IconEllipsis = "\u2026"; // horizontal ellipsis … (U+2026)

// ── Metadata Icons ──────────────────────────────────────────────────

export const IconBranch = "\uE0A0"; // nf-pl-branch (U+E0A0)
export const IconChat = "\uF086"; // nf-fa-comments (U+F086)
export const IconClock = "\uF017"; // nf-fa-clock (U+F017)
export const IconToken = "\uEDE8"; // nf-fa-coins (U+EDE8)
export const IconSession = "\u{F0237}"; // nf-md-fingerprint (U+F0237)
export const IconDot = "\u00B7"; // middle dot · (U+00B7)

// ── Task Status Icons ───────────────────────────────────────────────

export const IconTaskDone = "\u2713"; // check mark ✓ (U+2713)
export const IconTaskActive = "\u27F3"; // clockwise arrow ⟳ (U+27F3)
export const IconTaskPending = "\u25CB"; // white circle ○ (U+25CB)
export const IconTaskCancelled = "\u2717"; // ballot x ✗ (U+2717)

// ── Box Drawing / Separators ────────────────────────────────────────

export const IconBarSingle = "\u2502"; // │ (U+2502)
export const IconBarDouble = "\u2503"; // ┃ (U+2503)
export const IconHRule = "\u2500"; // ─ (U+2500)
export const IconTreeBranch = "\u2514"; // └ (U+2514)

// ── Activity Indicators ─────────────────────────────────────────────

export const IconOngoingDot = "\u25CF"; // filled circle ● (U+25CF)
export const IconBead = "\uEABC"; // nf-cod-circle (U+EABC)

// ── Project Tree ────────────────────────────────────────────────────

export const IconSelected2 = "\u25B8"; // small right triangle ▸ (U+25B8)
export const IconGroup = "\u2483"; // ⑃ (U+2483)

// ── Spawn (Ink-only, no Go equivalent) ──────────────────────────────

export const IconSpawn = "\uF061"; // nf-fa-arrow_right (U+F061) — reuse drill-down for spawns
