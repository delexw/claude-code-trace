import { describe, it, expect } from "vitest";
import { renderHook } from "@testing-library/react";
import { useRef } from "react";
import { useAutoScroll } from "./useAutoScroll";

function mockScrollableElement(scrollTop: number, scrollHeight: number, clientHeight: number) {
  const el = document.createElement("div");
  Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(el, "scrollTop", { value: scrollTop, writable: true, configurable: true });
  Object.defineProperty(el, "clientHeight", { value: clientHeight, configurable: true });
  return el;
}

describe("useAutoScroll", () => {
  it("returns a ref", () => {
    const { result } = renderHook(() => useAutoScroll(0));
    expect(result.current).toHaveProperty("current");
  });

  it("auto-scrolls when item count increases and near bottom", () => {
    const { result, rerender } = renderHook(({ count }) => useAutoScroll<HTMLDivElement>(count), {
      initialProps: { count: 1 },
    });

    const el = mockScrollableElement(400, 500, 100);
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ count: 2 });
    expect(el.scrollTop).toBe(500);
  });

  it("does not auto-scroll when user has scrolled up", () => {
    const { result, rerender } = renderHook(({ count }) => useAutoScroll<HTMLDivElement>(count), {
      initialProps: { count: 1 },
    });

    const el = mockScrollableElement(200, 1000, 100);
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ count: 2 });
    expect(el.scrollTop).toBe(200);
  });

  it("does not auto-scroll when count stays the same", () => {
    const { result, rerender } = renderHook(({ count }) => useAutoScroll<HTMLDivElement>(count), {
      initialProps: { count: 3 },
    });

    const el = mockScrollableElement(400, 500, 100);
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ count: 3 });
    expect(el.scrollTop).toBe(400);
  });

  it("works with an existing ref passed in", () => {
    const { result, rerender } = renderHook(
      ({ count }) => {
        const ref = useRef<HTMLDivElement>(null);
        useAutoScroll(count, ref);
        return ref;
      },
      { initialProps: { count: 1 } },
    );

    const el = mockScrollableElement(400, 500, 100);
    Object.defineProperty(result.current, "current", { value: el, writable: true });

    rerender({ count: 2 });
    expect(el.scrollTop).toBe(500);
  });
});
