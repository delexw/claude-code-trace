/** Which source the picker shows: every enabled plugin ("all") or one plugin id.
 *  Persisted in localStorage so it survives reloads. Default "all". */
export const SOURCE_FILTER_KEY = "cct.sourceFilter";
export const ALL_SOURCES = "all";

export function loadSourceFilter(): string {
  if (typeof localStorage === "undefined") return ALL_SOURCES;
  return localStorage.getItem(SOURCE_FILTER_KEY) || ALL_SOURCES;
}

export function saveSourceFilter(filter: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(SOURCE_FILTER_KEY, filter);
  } catch {
    // Storage may be full or disabled; the in-memory choice still applies.
  }
}
