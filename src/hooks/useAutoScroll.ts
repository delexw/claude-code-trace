import { useRef, useEffect, useLayoutEffect, type RefObject } from "react";

/**
 * Auto-scrolls a container to the bottom when content changes,
 * but only if the user was already near the bottom before the update.
 *
 * Triggers on both new items (count increase) and content updates
 * (changeSignal). Uses smooth scrolling for a polished experience.
 */
export function useAutoScroll<T extends HTMLElement>(
  itemCount: number,
  existingRef?: RefObject<T | null>,
  threshold = 150,
) {
  const ownRef = useRef<T>(null);
  const ref = existingRef ?? ownRef;
  const prevCountRef = useRef(itemCount);
  const isNearBottomRef = useRef(true);

  // Track near-bottom state via scroll events.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const checkNearBottom = () => {
      isNearBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
    };

    checkNearBottom();

    el.addEventListener("scroll", checkNearBottom, { passive: true });
    return () => el.removeEventListener("scroll", checkNearBottom);
  }, [ref, threshold]);

  // When item count increases → smooth scroll if near bottom.
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && itemCount > prevCountRef.current && isNearBottomRef.current) {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    }
    prevCountRef.current = itemCount;
  }, [itemCount, ref]);

  // Also scroll when content changes (same count but content grew).
  // Use a MutationObserver to detect DOM changes within the container.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const observer = new MutationObserver(() => {
      if (isNearBottomRef.current) {
        el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
      }
    });

    observer.observe(el, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [ref]);

  return ref;
}
