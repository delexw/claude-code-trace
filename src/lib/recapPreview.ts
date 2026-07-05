/** Whether the picker uses a session's end-of-session recap as its preview.
 *  Persisted in localStorage so it survives reloads. Default ON. */
export const RECAP_PREVIEW_KEY = "cct.recapPreview";
export const DEFAULT_RECAP_PREVIEW = true;

export function loadRecapPreview(): boolean {
  if (typeof localStorage === "undefined") return DEFAULT_RECAP_PREVIEW;
  const raw = localStorage.getItem(RECAP_PREVIEW_KEY);
  if (raw === null) return DEFAULT_RECAP_PREVIEW;
  return raw !== "false";
}

export function saveRecapPreview(on: boolean): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(RECAP_PREVIEW_KEY, String(on));
  } catch {
    // Storage may be full or disabled; the in-memory setting still applies.
  }
}
