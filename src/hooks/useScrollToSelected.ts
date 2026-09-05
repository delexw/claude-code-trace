import { useRef, useEffect } from "react";

/**
 * Keep the selected list item in view by scrolling ITS OWN nearest scroll
 * container — never the page.
 *
 * We deliberately do NOT use `element.scrollIntoView()`: that scrolls *every*
 * scrollable ancestor, and an `overflow: hidden` element is still
 * programmatically scrollable. The app shell (`.app` / `#root` / `body`) is
 * `overflow: hidden`, so `scrollIntoView` would scroll the shell and push the
 * InfoBar + ViewToolbar off the top — with no scrollbar to bring them back
 * (both bars vanish after navigating). Instead we locate the nearest real
 * scroll container and adjust only its `scrollTop`; if there is no dedicated
 * scroll container, we do nothing rather than risk scrolling the shell.
 */
export function useScrollToSelected(selected: number) {
  const ref = useRef<HTMLDivElement>(null);
  // The selection this hook last scrolled for. Callers move `ref` to whichever
  // item is selected, and refs aren't reactive, so the selected index is the
  // signal that the ref now points at a different element.
  const scrolledFor = useRef<number | null>(null);

  useEffect(() => {
    if (scrolledFor.current === selected) return;
    scrolledFor.current = selected;

    const el = ref.current;
    if (!el) return;

    // Nearest ancestor that actually scrolls (overflow auto/scroll on Y).
    let container: HTMLElement | null = el.parentElement;
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

    // No dedicated scroll container → nothing to do. Falling back to
    // scrollIntoView here would scroll the overflow:hidden app shell.
    if (!container || container === document.body) return;

    const elRect = el.getBoundingClientRect();
    const containerRect = container.getBoundingClientRect();

    if (elRect.top < containerRect.top || el.offsetHeight > container.clientHeight) {
      // Above the container, or taller than it → align the element top to the
      // container top (block: "start").
      container.scrollTop += elRect.top - containerRect.top;
    } else if (elRect.bottom > containerRect.bottom) {
      // Below the container → bring it just into view (block: "nearest").
      container.scrollTop += elRect.bottom - containerRect.bottom;
    }
    // Already fully visible → no-op.
  }, [selected]);

  return ref;
}
