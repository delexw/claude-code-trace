/**
 * Global UI zoom ("font size") support.
 *
 * The app's CSS is entirely px-based, so changing a root `font-size` would not
 * scale most of the UI. Instead we apply the CSS `zoom` property to the root
 * element, which reflows the whole layout — the equivalent of the browser's
 * built-in zoom — and is supported across the WebViews Tauri uses (WebKit,
 * WebView2, WebKitGTK) as well as plain browsers in web mode.
 *
 * The chosen scale is persisted in `localStorage` so it survives reloads and
 * applies on both desktop and web.
 */

export const FONT_SCALE_KEY = "cct.fontScale";

export const DEFAULT_FONT_SCALE = 1;

export const MIN_FONT_SCALE = 0.8;
export const MAX_FONT_SCALE = 2;

/** Preset zoom levels offered in the settings selector (1 = 100%). */
export const FONT_SCALE_PRESETS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;

/** Clamp an arbitrary number into the supported zoom range, falling back to the
 * default when the value is not a finite number. */
export function clampFontScale(scale: number): number {
  if (!Number.isFinite(scale)) return DEFAULT_FONT_SCALE;
  return Math.min(MAX_FONT_SCALE, Math.max(MIN_FONT_SCALE, scale));
}

/** Read the persisted zoom level, returning the default when unset, malformed,
 * or when `localStorage` is unavailable (e.g. SSR / privacy mode). */
export function readStoredFontScale(): number {
  if (typeof localStorage === "undefined") return DEFAULT_FONT_SCALE;
  const raw = localStorage.getItem(FONT_SCALE_KEY);
  if (raw === null) return DEFAULT_FONT_SCALE;
  return clampFontScale(Number.parseFloat(raw));
}

/** Persist the zoom level, ignoring write failures (quota / disabled storage). */
export function storeFontScale(scale: number): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(FONT_SCALE_KEY, String(scale));
  } catch {
    // Storage may be full or disabled; the in-memory scale still applies.
  }
}

/** Apply the zoom level to the document root. No-op when there is no DOM. */
export function applyFontScale(scale: number): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.zoom = String(clampFontScale(scale));
}

/** Format a scale as a percentage label, e.g. `1.25` -> `"125%"`. */
export function formatFontScale(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}
