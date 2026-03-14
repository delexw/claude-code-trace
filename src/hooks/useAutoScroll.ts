import { useRef, useEffect, type RefObject } from "react";

/**
 * Auto-scrolls a container to the bottom when `itemCount` increases,
 * but only if the user is already near the bottom (within `threshold` px).
 * Optionally accepts an existing ref to attach to instead of creating a new one.
 */
export function useAutoScroll<T extends HTMLElement>(
  itemCount: number,
  existingRef?: RefObject<T | null>,
  threshold = 150,
) {
  const ownRef = useRef<T>(null);
  const ref = existingRef ?? ownRef;
  const prevCountRef = useRef(itemCount);

  useEffect(() => {
    const el = ref.current;
    if (el && itemCount > prevCountRef.current) {
      const isNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
      if (isNearBottom) {
        el.scrollTop = el.scrollHeight;
      }
    }
    prevCountRef.current = itemCount;
  }, [itemCount, threshold, ref]);

  return ref;
}
