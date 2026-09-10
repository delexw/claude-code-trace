/**
 * Safe wrapper around Tauri's `listen`.
 *
 * In the Tauri webview it delegates to the real event system.
 * In a plain browser it subscribes to the Rust backend's SSE endpoint.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";
import { isTauri } from "./isTauri";
import { API_BASE } from "./config";
import { onApiTokenChange, withTokenQuery } from "./apiToken";

export type UnlistenFn = () => void;

/** Shared SSE connection — lazily created, ref-counted. */
let sseSource: EventSource | null = null;
let sseRefCount = 0;

/** Every listener currently registered, by event name, so a replacement
 * connection (after a credential reissue or a closed stream) can re-attach them. */
const registered = new Map<string, Set<EventListener>>();

/**
 * Reopen schedule for a stream the browser has given up on.
 *
 * `EventSource` retries network failures on its own, but a non-200 reply makes
 * it *fail the connection*: `readyState` goes to `CLOSED`, one `error` event
 * fires, and it never tries again. Before `/api/events` required a credential
 * that could not happen; now it does whenever a reconnect is refused with a
 * 401 — the browser reconnects with the URL (and, in Docker, whatever cookie)
 * the stream was opened with, so a backend restart, a sleep/wake or a
 * suspended tab that lands after the credential changed leaves the stream
 * dead while the page looks connected: no `session-update`, no
 * `picker-refresh`, no live tail. So a `CLOSED` stream is reopened here with
 * the *current* credential, backing off between attempts, for as long as
 * anything is listening.
 */
export const SSE_REOPEN_MIN_MS = 1_000;
export const SSE_REOPEN_MAX_MS = 30_000;
let reopenDelay = SSE_REOPEN_MIN_MS;
let reopenTimer: ReturnType<typeof setTimeout> | null = null;

function cancelReopen(): void {
  if (reopenTimer !== null) {
    clearTimeout(reopenTimer);
    reopenTimer = null;
  }
}

function openSource(): EventSource {
  // `EventSource` can't set headers, so the client credential rides in the
  // query string (see lib/apiToken.ts). Empty in Docker, where the cookie is used.
  const source = new EventSource(withTokenQuery(`${API_BASE}/api/events`));
  for (const [event, handlers] of registered) {
    for (const handler of handlers) source.addEventListener(event, handler);
  }
  source.addEventListener("open", () => {
    // A stream that made it through is proof the credential is good again.
    if (source === sseSource) reopenDelay = SSE_REOPEN_MIN_MS;
  });
  source.addEventListener("error", () => onSourceError(source));
  return source;
}

function onSourceError(source: EventSource): void {
  // The last gasp of a stream already replaced or released — nothing to do.
  if (source !== sseSource || sseRefCount <= 0) return;
  // `CONNECTING`: a network failure the browser is retrying by itself.
  if (source.readyState !== EventSource.CLOSED) return;
  if (reopenTimer !== null) return;
  reopenTimer = setTimeout(() => {
    reopenTimer = null;
    if (sseSource !== source || sseRefCount <= 0) return;
    sseSource = openSource();
  }, reopenDelay);
  reopenDelay = Math.min(reopenDelay * 2, SSE_REOPEN_MAX_MS);
}

function ensureSse(): EventSource {
  if (!sseSource || sseSource.readyState === EventSource.CLOSED) {
    cancelReopen();
    sseSource = openSource();
  }
  sseRefCount++;
  return sseSource;
}

function releaseSse(): void {
  sseRefCount--;
  if (sseRefCount <= 0) {
    cancelReopen();
    sseSource?.close();
    sseSource = null;
    sseRefCount = 0;
  }
}

/**
 * Drop the current SSE connection and open a fresh one carrying the *current*
 * token, re-attaching every registered listener. The old stream was
 * authenticated with the old token and would silently die on its next
 * reconnect. No-op when nothing is listening.
 */
export function reconnectSse(): void {
  if (!sseSource) return;
  // A new credential is the likeliest cure for a refused stream: reopen now,
  // and let the next failure (if any) start the backoff from scratch.
  cancelReopen();
  reopenDelay = SSE_REOPEN_MIN_MS;
  sseSource.close();
  sseSource = openSource();
}

// Wherever the credential changes — this tab's Reissue, or a reissue by another
// client pushed here over HMR (see lib/apiToken.ts) — the stream follows it.
onApiTokenChange(() => reconnectSse());

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    return tauriListen<T>(event, handler);
  }

  const source = ensureSse();
  const onMessage = ((e: MessageEvent) => {
    try {
      const payload = JSON.parse(e.data) as T;
      handler({ payload });
    } catch {
      // ignore malformed events
    }
  }) as EventListener;
  source.addEventListener(event, onMessage);
  let handlers = registered.get(event);
  if (!handlers) {
    handlers = new Set();
    registered.set(event, handlers);
  }
  handlers.add(onMessage);

  return () => {
    // Remove from whichever connection is live now — it may have been
    // replaced by `reconnectSse` since this listener was attached.
    sseSource?.removeEventListener(event, onMessage);
    const set = registered.get(event);
    set?.delete(onMessage);
    if (set && set.size === 0) registered.delete(event);
    releaseSse();
  };
}
