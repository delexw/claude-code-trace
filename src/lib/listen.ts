/**
 * Safe wrapper around Tauri's `listen`.
 *
 * In the Tauri webview it delegates to the real event system.
 * In a plain browser it subscribes to the Rust backend's SSE endpoint.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";
import { isTauri } from "./isTauri";
import { API_BASE } from "./config";
import { withTokenQuery } from "./apiToken";

export type UnlistenFn = () => void;

/** Shared SSE connection — lazily created, ref-counted. */
let sseSource: EventSource | null = null;
let sseRefCount = 0;

/** Every listener currently registered, by event name, so a replacement
 * connection (after a token rotation or a closed stream) can re-attach them. */
const registered = new Map<string, Set<EventListener>>();

function openSource(): EventSource {
  // `EventSource` can't set headers, so the API token rides in the query
  // string (see lib/apiToken.ts). Empty in Docker, where the cookie is used.
  const source = new EventSource(withTokenQuery(`${API_BASE}/api/events`));
  for (const [event, handlers] of registered) {
    for (const handler of handlers) source.addEventListener(event, handler);
  }
  return source;
}

function ensureSse(): EventSource {
  if (!sseSource || sseSource.readyState === EventSource.CLOSED) {
    sseSource = openSource();
  }
  sseRefCount++;
  return sseSource;
}

function releaseSse(): void {
  sseRefCount--;
  if (sseRefCount <= 0 && sseSource) {
    sseSource.close();
    sseSource = null;
    sseRefCount = 0;
  }
}

/**
 * Drop the current SSE connection and open a fresh one carrying the *current*
 * token, re-attaching every registered listener. Called after Settings →
 * Regenerate: the old stream was authenticated with the old token and would
 * silently die on its next reconnect. No-op when nothing is listening.
 */
export function reconnectSse(): void {
  if (!sseSource) return;
  sseSource.close();
  sseSource = openSource();
}

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
