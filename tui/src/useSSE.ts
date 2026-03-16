import { useEffect, useRef } from "react";
import { EventSource } from "eventsource";

import { API_BASE } from "./api.js";

/** Shared SSE connection — lazily created, ref-counted (matches web's listen.ts pattern). */
let sharedSource: EventSource | null = null;
let refCount = 0;

function acquireSource(): EventSource {
  if (!sharedSource || sharedSource.readyState === EventSource.CLOSED) {
    sharedSource = new EventSource(`${API_BASE}/api/events`);
  }
  refCount++;
  return sharedSource;
}

function releaseSource(): void {
  refCount--;
  if (refCount <= 0 && sharedSource) {
    sharedSource.close();
    sharedSource = null;
    refCount = 0;
  }
}

/** Subscribe to SSE events from the Rust backend. Shares a single EventSource connection. */
export function useSSE<T>(event: string, handler: (payload: T) => void) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    const source = acquireSource();
    const onMessage = (e: MessageEvent) => {
      try {
        handlerRef.current(JSON.parse(e.data) as T);
      } catch {
        // ignore malformed events
      }
    };
    source.addEventListener(event, onMessage as EventListener);
    return () => {
      source.removeEventListener(event, onMessage as EventListener);
      releaseSource();
    };
  }, [event]);
}
