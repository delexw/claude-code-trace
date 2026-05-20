import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "../lib/listen";

/**
 * Subscribe to a Tauri event with automatic setup/teardown and cancellation safety.
 * The handler is kept in a ref so it always sees fresh closures without re-subscribing.
 */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void | Promise<void>) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    let cancelled = false;

    const setupListener = async () => {
      const unlisten = await listen<T>(event, (e) => {
        if (!cancelled) handlerRef.current(e.payload);
      });

      if (!cancelled) {
        unlistenRef.current = unlisten;
      } else {
        unlisten();
      }
    };

    void setupListener();

    return () => {
      cancelled = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, [event]);
}
