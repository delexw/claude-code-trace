import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useScrollToSelected } from "./useScrollToSelected";

describe("useScrollToSelected", () => {
  // jsdom default window.innerHeight = 0, so el.offsetHeight > containerHeight is
  // always true. This lets us test the "tall element → block: start" path, but
  // makes the "already visible" and "below + fits → nearest" branches
  // unreachable without a real scrolling container.
  beforeEach(() => {
    Object.defineProperty(window, "innerHeight", { value: 0, writable: true });
  });

  it("returns a ref object", () => {
    const { result } = renderHook(() => useScrollToSelected(0));
    expect(result.current).toHaveProperty("current");
  });

  it("scrolls to start when element is above the container", () => {
    const scrollIntoView = vi.fn();
    const getBoundingClientRect = vi.fn(() => ({
      top: -100,
      bottom: 50,
      left: 0,
      right: 100,
      width: 100,
      height: 150,
    }));
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });

    Object.defineProperty(result.current, "current", {
      value: { scrollIntoView, getBoundingClientRect, offsetHeight: 150 },
      writable: true,
    });

    rerender({ dep: 1 });

    expect(scrollIntoView).toHaveBeenCalledWith({ block: "start" });
  });

  // In jsdom (window.innerHeight = 0), any element with offsetHeight > 0 satisfies
  // el.offsetHeight > containerHeight, so both "tall" and "below + fits" paths
  // collapse into the first branch. We test the tall-element case here.
  it("scrolls to start when element is taller than the container", () => {
    const scrollIntoView = vi.fn();
    const getBoundingClientRect = vi.fn(() => ({
      top: 50,
      bottom: 250,
      left: 0,
      right: 100,
      width: 100,
      height: 200,
    }));
    const { result, rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });

    Object.defineProperty(result.current, "current", {
      value: { scrollIntoView, getBoundingClientRect, offsetHeight: 200 },
      writable: true,
    });

    rerender({ dep: 1 });

    expect(scrollIntoView).toHaveBeenCalledWith({ block: "start" });
  });

  it("does not throw when ref.current is null", () => {
    const { rerender } = renderHook(({ dep }) => useScrollToSelected(dep), {
      initialProps: { dep: 0 },
    });

    expect(() => rerender({ dep: 1 })).not.toThrow();
  });
});
