/**
 * WebKit (Tauri's macOS webview engine) retains render-tree/layout memory
 * across repeated large DOM swaps within this long-lived single-page app —
 * measured via `vmmap`: physical footprint climbed steadily across repeated
 * session switches even though JS heap and DOM node counts stayed flat.
 * There's no public WKWebView API to reclaim it; a full reload is the only
 * reliable reset. This tracks how many sessions have been opened this page
 * lifetime and, once a threshold is hit, saves which session was open and
 * reloads, so the reload restores continuity instead of dumping the user
 * back to an empty picker.
 */

import { inFlightInvokeCount } from "./invoke";

const STORAGE_KEY = "cctrace.pendingRestore";

/** Reload after this many session opens (empirically, memory growth is
 * steady enough that this keeps the webview's footprint from compounding
 * over a long working session without reloading disruptively often). */
export const RELOAD_AFTER_N_SWITCHES = 25;

export interface PendingRestore {
  sessionPath: string;
}

export function shouldRecycle(switchCount: number): boolean {
  return switchCount >= RELOAD_AFTER_N_SWITCHES;
}

export function saveRestoreState(state: PendingRestore): void {
  sessionStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

/** Reads and clears the pending restore state, if any (survives a
 * `location.reload()` since it uses `sessionStorage`, not module state). */
export function takeRestoreState(): PendingRestore | null {
  const raw = sessionStorage.getItem(STORAGE_KEY);
  if (!raw) return null;
  sessionStorage.removeItem(STORAGE_KEY);
  try {
    const parsed = JSON.parse(raw) as PendingRestore;
    return typeof parsed.sessionPath === "string" ? parsed : null;
  } catch {
    return null;
  }
}

/** Waits until no `invoke` call is in flight, polling every `pollMs` up to
 * `timeoutMs`. Reloading on macOS while a Tauri IPC call is still pending is a
 * known crash cause (tauri-apps/tauri#9933) — this app always has some async
 * invoke in flight (session loads, page fetches), so reloading unconditionally
 * risked never visibly happening. The timeout keeps a call that never
 * resolves from blocking the reload forever. */
async function waitForQuietInvokes(timeoutMs = 3000, pollMs = 50): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (inFlightInvokeCount() > 0 && Date.now() < deadline) {
    // Sequential polling by design: each wait must complete before rechecking
    // the (externally mutated) in-flight count — nothing to run in parallel.
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, pollMs));
  }
}

export async function reloadWebview(): Promise<void> {
  await waitForQuietInvokes();
  window.location.reload();
}
