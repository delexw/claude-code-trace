/**
 * Centralized icon/glyph definitions for the TUI.
 *
 * Uses standard Unicode symbols that render in ANY terminal font —
 * no Nerd Font or patched font required. All codepoints are in the
 * Basic Multilingual Plane (BMP) and supported by virtually all fonts.
 *
 * Previous version used Nerd Font PUA glyphs (U+E000–U+F8FF, U+F0000+)
 * which rendered as empty boxes (□) in terminals without Nerd Font.
 */

// ── Role Icons ──────────────────────────────────────────────────────

export const IconClaude = "\u2726"; // four-pointed star ✦ (U+2726)
export const IconUser = "\u25CF"; // filled circle ● (U+25CF)
export const IconSystem = "$"; // dollar sign (standard terminal prompt)
export const IconSystemErr = "!"; // exclamation mark

// ── Item Type Icons ─────────────────────────────────────────────────

export const IconThinking = "\u25C6"; // black diamond ◆ (U+25C6)
export const IconOutput = "\u25AA"; // black small square ▪ (U+25AA)
export const IconTool = "\u2699"; // gear ⚙ (U+2699)
export const IconSubagent = "\u2726"; // four-pointed star ✦ (U+2726)
export const IconTeammate = "\u25C8"; // white diamond containing black small diamond ◈ (U+25C8)
export const IconHook = "\u26A1"; // high voltage ⚡ (U+26A1)

// ── Navigation/Cursor Icons ─────────────────────────────────────────

export const IconSelected = "\u2502"; // box drawing vertical │ (U+2502)
export const IconExpanded = "\u25BE"; // down-pointing small triangle ▾ (U+25BE)
export const IconCollapsed = "\u25B8"; // right-pointing small triangle ▸ (U+25B8)
export const IconDrillDown = "\u2192"; // rightwards arrow → (U+2192)
export const IconEllipsis = "\u2026"; // horizontal ellipsis … (U+2026)

// ── Metadata Icons ──────────────────────────────────────────────────
// Use simple ASCII/Latin-1 characters that render in any font.

export const IconBranch = "*"; // asterisk — conventional git branch marker
export const IconChat = "#"; // hash — turn/message count
export const IconClock = "~"; // tilde — relative time
export const IconToken = "$"; // dollar — token/cost
export const IconSession = "@"; // at-sign — session identity
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
export const IconBead = "\u25CF"; // filled circle ● (U+25CF) — same as ongoing

// ── Project Tree ────────────────────────────────────────────────────

export const IconSelected2 = "\u25B8"; // right-pointing small triangle ▸ (U+25B8)
export const IconGroup = "\u2261"; // triple horizontal bar ≡ (U+2261)

// ── Spawn ───────────────────────────────────────────────────────────

export const IconSpawn = "\u2192"; // rightwards arrow → (U+2192)
