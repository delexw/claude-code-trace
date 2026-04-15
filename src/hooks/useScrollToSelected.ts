import { useRef, useEffect } from "react";

export function useScrollToSelected(dep: number) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    let container = el.parentElement;
    while (container && container !== document.body) {
      const style = window.getComputedStyle(container);
      if (
        style.overflowY === "auto" ||
        style.overflowY === "scroll" ||
        style.overflow === "auto" ||
        style.overflow === "scroll"
      ) {
        break;
      }
      container = container.parentElement;
    }

    const containerHeight =
      container && container !== document.body ? container.clientHeight : window.innerHeight;

    // Use getBoundingClientRect to determine if the element is above the viewport
    const elRect = el.getBoundingClientRect();
    const containerRect =
      container && container !== document.body
        ? container.getBoundingClientRect()
        : new DOMRect(0, 0, window.innerWidth, window.innerHeight);

    // If the top is above the container, or the element is taller than the
    // container, align to the top so the header stays visible.
    if (elRect.top < containerRect.top || el.offsetHeight > containerHeight) {
      el.scrollIntoView({ block: "start" });
    } else if (elRect.bottom > containerRect.bottom) {
      // Below the container → bring into view with nearest alignment.
      el.scrollIntoView({ block: "nearest" });
    }
    // Already fully visible → no-op.
  }, [dep]);

  return ref;
}
