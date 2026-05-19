import { useCallback, useEffect, useRef, useState } from "react";

interface Options {
  /** Time to wait after a visibility change before firing `onChange` (ms). */
  debounceMs?: number;
  /** Periodic re-emit interval while visible set is non-empty (ms). 0 disables. */
  heartbeatMs?: number;
}

/**
 * Track which session cards are visible in the picker viewport via
 * IntersectionObserver. Returns a `register(path)` factory that produces a
 * callback ref to attach to each card.
 *
 * `onChange` fires (debounced) when the visible set changes, and again on a
 * periodic heartbeat while any cards remain visible — that's the cue the
 * caller uses to re-fetch fresh session info for what the user is looking at.
 *
 * Implementation note: the observer is created lazily in a `useState`
 * initializer so it exists during the commit phase when card refs are first
 * attached, not later in a `useEffect`. Otherwise the very first batch of
 * cards never gets observed.
 */
export function useVisibleSessions(
  onChange: (paths: string[]) => void,
  { debounceMs = 150, heartbeatMs = 2000 }: Options = {},
) {
  const visibleRef = useRef<Set<string>>(new Set());
  const elsRef = useRef<Map<string, Element>>(new Map());
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handlerRef = useRef(onChange);
  handlerRef.current = onChange;
  const debounceMsRef = useRef(debounceMs);
  debounceMsRef.current = debounceMs;

  const [observer] = useState<IntersectionObserver | null>(() => {
    if (typeof IntersectionObserver === "undefined") return null;
    return new IntersectionObserver((entries) => {
      let changed = false;
      for (const entry of entries) {
        const path = (entry.target as HTMLElement).dataset.path;
        if (!path) continue;
        if (entry.isIntersecting) {
          if (!visibleRef.current.has(path)) {
            visibleRef.current.add(path);
            changed = true;
          }
        } else if (visibleRef.current.has(path)) {
          visibleRef.current.delete(path);
          changed = true;
        }
      }
      if (!changed) return;
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = setTimeout(() => {
        handlerRef.current(Array.from(visibleRef.current));
      }, debounceMsRef.current);
    });
  });

  // Cleanup on unmount. Refs go out of scope with the hook, no need to clear them.
  useEffect(() => {
    const timerRef = debounceTimerRef;
    return () => {
      observer?.disconnect();
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [observer]);

  // Heartbeat: re-emit visible set periodically so callers can refresh stats.
  useEffect(() => {
    if (heartbeatMs <= 0) return;
    const id = setInterval(() => {
      if (visibleRef.current.size > 0) {
        handlerRef.current(Array.from(visibleRef.current));
      }
    }, heartbeatMs);
    return () => clearInterval(id);
  }, [heartbeatMs]);

  /** Returns a callback ref for a given session path. */
  const register = useCallback(
    (path: string) => {
      return (el: HTMLElement | null) => {
        if (!observer) return;
        const prev = elsRef.current.get(path);
        if (prev && prev !== el) {
          observer.unobserve(prev);
          visibleRef.current.delete(path);
          elsRef.current.delete(path);
        }
        if (el) {
          el.dataset.path = path;
          observer.observe(el);
          elsRef.current.set(path, el);
        }
      };
    },
    [observer],
  );

  return register;
}
