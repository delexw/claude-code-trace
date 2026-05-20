/**
 * Safe wrapper around Tauri's `openUrl`.
 *
 * In the Tauri webview it opens the URL via the system browser.
 * In a plain browser it uses window.open.
 */
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { isTauri } from "./isTauri";

export function openUrl(url: string): void {
  if (isTauri) {
    void tauriOpenUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}
