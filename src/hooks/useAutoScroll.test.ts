import { describe, it, expect, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useRef } from "react";
import { useAutoScroll } from "./useAutoScroll";

function mockScrollableElement(scrollTop: number, scrollHeight: number, clientHeight: number) {
  const el = document.createElement("div");
  Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(el, "scrollTop", { value: scrollTop, writable: true, configurable: true });
  Object.defineProperty(el, "clientHeight", { value: clientHeight, configurable: true });
  // Mock scrollTo — jsdom doesn't implement it properly
  el.scrollTo = vi.fn((...args: unknown[]) => {
    const opts =
      typeof args[0] === "object" ? (args[0] as ScrollToOptions) : { top: args[1] as number };
    Object.defineProperty(el, "scrollTop", {
      value: opts.top ?? 0,
      writable: true,
      configurable: true,
    });
  }) as unknown as typeof el.scrollTo;
  return el;
}

/**
 * Set up the hook with a mock element already attached.
 * Uses an existing ref so the element is available from the first render.
 */
function setup(el: HTMLElement, initialCount: number) {
  const refObj = { current: el as HTMLDivElement };
  const { rerender } = renderHook(
    ({ count }) => {
      const ref = useRef<HTMLDivElement>(el as HTMLDivElement);
      ref.current = refObj.current as HTMLDivElement;
      useAutoScroll(count, ref);
      return ref;
    },
    { initialProps: { count: initialCount } },
  );
  return { el, rerender };
}

describe("useAutoScroll", () => {
  it("returns a ref", () => {
    const { result } = renderHook(() => useAutoScroll(0));
    expect(result.current).toHaveProperty("current");
  });

  it("auto-scrolls when item count increases and was near bottom", () => {
    // distance = 500 - 400 - 100 = 0 (at bottom)
    const el = mockScrollableElement(400, 500, 100);
    const { rerender } = setup(el, 1);

    rerender({ count: 2 });
    expect(el.scrollTo).toHaveBeenCalledWith({ top: 500, behavior: "smooth" });
  });

  it("does not auto-scroll when user has scrolled up", () => {
    // distance = 1000 - 200 - 100 = 700 (far from bottom)
    const el = mockScrollableElement(200, 1000, 100);
    const { rerender } = setup(el, 1);

    rerender({ count: 2 });
    // scrollTo should not be called (or only from MutationObserver which we don't trigger here)
    expect(el.scrollTop).toBe(200);
  });

  it("does not auto-scroll when count stays the same", () => {
    const el = mockScrollableElement(400, 500, 100);
    const { rerender } = setup(el, 3);

    const callsBefore = (el.scrollTo as ReturnType<typeof vi.fn>).mock.calls.length;
    rerender({ count: 3 });
    const callsAfter = (el.scrollTo as ReturnType<typeof vi.fn>).mock.calls.length;
    // No new scrollTo call from the count-based effect
    expect(callsAfter).toBe(callsBefore);
  });

  it("works with an existing ref passed in", () => {
    const el = mockScrollableElement(400, 500, 100);
    const refObj = { current: el as HTMLDivElement };
    const { rerender } = renderHook(
      ({ count }) => {
        const ref = useRef<HTMLDivElement>(el as HTMLDivElement);
        ref.current = refObj.current as HTMLDivElement;
        useAutoScroll(count, ref);
        return ref;
      },
      { initialProps: { count: 1 } },
    );

    rerender({ count: 2 });
    expect(el.scrollTo).toHaveBeenCalledWith({ top: 500, behavior: "smooth" });
  });

  it("defaults to near-bottom on initial render (no scroll event yet)", () => {
    const { result, rerender } = renderHook(({ count }) => useAutoScroll<HTMLDivElement>(count), {
      initialProps: { count: 1 },
    });

    const el = mockScrollableElement(0, 500, 500);
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ count: 2 });
    expect(el.scrollTo).toHaveBeenCalledWith({ top: 500, behavior: "smooth" });
  });

  it("auto-scrolls even when new content adds significant height", () => {
    const el = mockScrollableElement(400, 500, 100);
    const { rerender } = setup(el, 1);

    Object.defineProperty(el, "scrollHeight", { value: 2000, configurable: true });

    rerender({ count: 2 });
    expect(el.scrollTo).toHaveBeenCalledWith({ top: 2000, behavior: "smooth" });
  });

  it("resumes auto-scroll when user scrolls back to bottom", () => {
    const el = mockScrollableElement(200, 1000, 100);
    const { rerender } = setup(el, 1);

    // Should not scroll (far from bottom)
    rerender({ count: 2 });
    expect(el.scrollTop).toBe(200);

    // User scrolls to bottom
    Object.defineProperty(el, "scrollTop", { value: 900, writable: true, configurable: true });
    Object.defineProperty(el, "scrollHeight", { value: 1000, configurable: true });
    act(() => el.dispatchEvent(new Event("scroll")));

    rerender({ count: 3 });
    expect(el.scrollTo).toHaveBeenCalledWith({ top: 1000, behavior: "smooth" });
  });
});
