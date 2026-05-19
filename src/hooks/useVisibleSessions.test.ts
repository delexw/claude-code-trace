import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useVisibleSessions } from "./useVisibleSessions";

type Callback = (entries: IntersectionObserverEntry[], observer: IntersectionObserver) => void;

class FakeIO {
  static last: FakeIO | null = null;
  cb: Callback;
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
  takeRecords = vi.fn(() => [] as IntersectionObserverEntry[]);
  root = null;
  rootMargin = "";
  thresholds: number[] = [];

  constructor(cb: Callback) {
    this.cb = cb;
    FakeIO.last = this;
  }

  /** Simulate the browser delivering visibility changes. */
  trigger(updates: { el: Element; isIntersecting: boolean }[]) {
    const entries = updates.map(
      (u) =>
        ({
          target: u.el,
          isIntersecting: u.isIntersecting,
          intersectionRatio: u.isIntersecting ? 1 : 0,
          boundingClientRect: new DOMRect(),
          intersectionRect: new DOMRect(),
          rootBounds: new DOMRect(),
          time: 0,
        }) as unknown as IntersectionObserverEntry,
    );
    this.cb(entries, this as unknown as IntersectionObserver);
  }
}

beforeEach(() => {
  FakeIO.last = null;
  (globalThis as { IntersectionObserver: unknown }).IntersectionObserver = FakeIO;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useVisibleSessions", () => {
  it("creates an IntersectionObserver on mount", () => {
    renderHook(() => useVisibleSessions(vi.fn()));
    expect(FakeIO.last).not.toBeNull();
  });

  it("registers an element via observer.observe and tags it with data-path", () => {
    const { result } = renderHook(() => useVisibleSessions(vi.fn()));
    const el = document.createElement("div");
    act(() => {
      result.current("/a/b.jsonl")(el);
    });
    expect(FakeIO.last!.observe).toHaveBeenCalledWith(el);
    expect(el.dataset.path).toBe("/a/b.jsonl");
  });

  it("fires onChange (debounced) when a card becomes visible", () => {
    const onChange = vi.fn();
    const { result } = renderHook(() => useVisibleSessions(onChange, { debounceMs: 100 }));
    const el = document.createElement("div");
    act(() => {
      result.current("/a/b.jsonl")(el);
    });

    act(() => {
      FakeIO.last!.trigger([{ el, isIntersecting: true }]);
    });
    expect(onChange).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(onChange).toHaveBeenCalledExactlyOnceWith(["/a/b.jsonl"]);
  });

  it("drops a path from the visible set when it leaves the viewport", () => {
    const onChange = vi.fn();
    const { result } = renderHook(() => useVisibleSessions(onChange, { debounceMs: 100 }));
    const el = document.createElement("div");
    act(() => {
      result.current("/a/b.jsonl")(el);
    });

    act(() => {
      FakeIO.last!.trigger([{ el, isIntersecting: true }]);
      vi.advanceTimersByTime(100);
    });
    onChange.mockClear();

    act(() => {
      FakeIO.last!.trigger([{ el, isIntersecting: false }]);
      vi.advanceTimersByTime(100);
    });
    expect(onChange).toHaveBeenCalledExactlyOnceWith([]);
  });

  it("coalesces rapid visibility changes within the debounce window", () => {
    const onChange = vi.fn();
    const { result } = renderHook(() => useVisibleSessions(onChange, { debounceMs: 100 }));
    const el1 = document.createElement("div");
    const el2 = document.createElement("div");
    act(() => {
      result.current("/a.jsonl")(el1);
      result.current("/b.jsonl")(el2);
    });

    act(() => {
      FakeIO.last!.trigger([{ el: el1, isIntersecting: true }]);
      vi.advanceTimersByTime(50);
      FakeIO.last!.trigger([{ el: el2, isIntersecting: true }]);
      vi.advanceTimersByTime(100);
    });

    expect(onChange).toHaveBeenCalledExactlyOnceWith(
      expect.arrayContaining(["/a.jsonl", "/b.jsonl"]),
    );
  });

  it("re-emits visible paths on the heartbeat interval", () => {
    const onChange = vi.fn();
    const { result } = renderHook(() =>
      useVisibleSessions(onChange, { debounceMs: 50, heartbeatMs: 1000 }),
    );
    const el = document.createElement("div");
    act(() => {
      result.current("/a.jsonl")(el);
      FakeIO.last!.trigger([{ el, isIntersecting: true }]);
      vi.advanceTimersByTime(50);
    });
    onChange.mockClear();

    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(onChange).toHaveBeenCalledExactlyOnceWith(["/a.jsonl"]);
  });

  it("skips the heartbeat when no cards are visible", () => {
    const onChange = vi.fn();
    renderHook(() => useVisibleSessions(onChange, { heartbeatMs: 1000 }));

    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("disconnects the observer on unmount", () => {
    const { unmount } = renderHook(() => useVisibleSessions(vi.fn()));
    const observer = FakeIO.last!;
    unmount();
    expect(observer.disconnect).toHaveBeenCalled();
  });

  it("unobserves the previous element when a path's ref changes", () => {
    const { result } = renderHook(() => useVisibleSessions(vi.fn()));
    const el1 = document.createElement("div");
    const el2 = document.createElement("div");
    const reg = result.current("/a.jsonl");

    act(() => {
      reg(el1);
      reg(el2);
    });

    expect(FakeIO.last!.unobserve).toHaveBeenCalledWith(el1);
    expect(FakeIO.last!.observe).toHaveBeenCalledWith(el2);
  });
});
