import { describe, it, expect, vi, afterEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useScrollToSelected } from "./useScrollToSelected";

type Rect = {
  top: number;
  bottom: number;
  left: number;
  right: number;
  width: number;
  height: number;
};

function rect(top: number, bottom: number): Rect {
  return { top, bottom, left: 0, right: 100, width: 100, height: bottom - top };
}

/** A scroll container (overflow-y: auto) with a mocked layout, holding a child
 *  element with a mocked rect. Returns both plus a scrollIntoView spy on the
 *  child so tests can assert we never fall back to it. */
function mount(opts: {
  containerRect: Rect;
  clientHeight: number;
  scrollTop: number;
  elRect: Rect;
  offsetHeight: number;
  /** When true, the wrapper is overflow:hidden (NOT a scroll container). */
  noScrollContainer?: boolean;
}) {
  const container = document.createElement("div");
  container.style.overflowY = opts.noScrollContainer ? "hidden" : "auto";
  container.getBoundingClientRect = () => opts.containerRect as DOMRect;
  Object.defineProperty(container, "clientHeight", {
    value: opts.clientHeight,
    configurable: true,
  });
  container.scrollTop = opts.scrollTop;

  const el = document.createElement("div");
  el.getBoundingClientRect = () => opts.elRect as DOMRect;
  Object.defineProperty(el, "offsetHeight", { value: opts.offsetHeight, configurable: true });
  const scrollIntoView = vi.fn();
  el.scrollIntoView = scrollIntoView;

  container.appendChild(el);
  document.body.appendChild(container);
  return { container, el, scrollIntoView };
}

describe("useScrollToSelected", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("returns a ref object", () => {
    const { result } = renderHook(() => useScrollToSelected(0));
    expect(result.current).toHaveProperty("current");
  });

  it("scrolls the container UP when the element is above it (block: start)", () => {
    const { container, el, scrollIntoView } = mount({
      containerRect: rect(100, 300),
      clientHeight: 200,
      scrollTop: 500,
      elRect: rect(50, 90), // top (50) < container top (100)
      offsetHeight: 40,
    });
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ dep: 1 });

    // 500 + (50 - 100) = 450
    expect(container.scrollTop).toBe(450);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("scrolls the container DOWN when the element is below it (block: nearest)", () => {
    const { container, el, scrollIntoView } = mount({
      containerRect: rect(100, 300),
      clientHeight: 200,
      scrollTop: 0,
      elRect: rect(320, 360), // bottom (360) > container bottom (300)
      offsetHeight: 40,
    });
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ dep: 1 });

    // 0 + (360 - 300) = 60
    expect(container.scrollTop).toBe(60);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("aligns to the top when the element is taller than the container", () => {
    const { container, el } = mount({
      containerRect: rect(100, 300),
      clientHeight: 200,
      scrollTop: 0,
      elRect: rect(150, 650),
      offsetHeight: 500, // > clientHeight (200)
    });
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ dep: 1 });

    // 0 + (150 - 100) = 50
    expect(container.scrollTop).toBe(50);
  });

  it("is a no-op when the element is already fully visible", () => {
    const { container } = mount({
      containerRect: rect(100, 300),
      clientHeight: 200,
      scrollTop: 0,
      elRect: rect(150, 250),
      offsetHeight: 100,
    });
    const el = container.firstElementChild as HTMLElement;
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ dep: 1 });

    expect(container.scrollTop).toBe(0);
  });

  it("never scrolls (or calls scrollIntoView) when there is no scroll container", () => {
    // Regression: the app shell (.app / #root / body) is overflow:hidden. With no
    // dedicated scroll container the hook must do NOTHING — never scrollIntoView,
    // which would scroll the shell and hide the header bars with no way back.
    const { el, scrollIntoView } = mount({
      containerRect: rect(100, 300),
      clientHeight: 200,
      scrollTop: 0,
      elRect: rect(-500, -400), // far above — scrollIntoView would definitely fire
      offsetHeight: 100,
      noScrollContainer: true,
    });
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    expect(() => rerender({ dep: 1 })).not.toThrow();
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("does not throw when ref.current is null", () => {
    const { rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });
    expect(() => rerender({ dep: 1 })).not.toThrow();
  });
});
