import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  shouldRecycle,
  saveRestoreState,
  takeRestoreState,
  reloadWebview,
  RELOAD_AFTER_N_SWITCHES,
} from "./webviewRecycle";
import * as invokeModule from "./invoke";

beforeEach(() => {
  sessionStorage.clear();
});

describe("shouldRecycle", () => {
  it("is false below the threshold", () => {
    expect(shouldRecycle(0)).toBe(false);
    expect(shouldRecycle(RELOAD_AFTER_N_SWITCHES - 1)).toBe(false);
  });

  it("is true at and above the threshold", () => {
    expect(shouldRecycle(RELOAD_AFTER_N_SWITCHES)).toBe(true);
    expect(shouldRecycle(RELOAD_AFTER_N_SWITCHES + 5)).toBe(true);
  });
});

describe("save/takeRestoreState", () => {
  it("round-trips a saved session path", () => {
    saveRestoreState({ sessionPath: "/proj/s.jsonl" });
    expect(takeRestoreState()).toEqual({ sessionPath: "/proj/s.jsonl" });
  });

  it("clears the state after reading it once", () => {
    saveRestoreState({ sessionPath: "/proj/s.jsonl" });
    takeRestoreState();
    expect(takeRestoreState()).toBeNull();
  });

  it("returns null when nothing was saved", () => {
    expect(takeRestoreState()).toBeNull();
  });

  it("returns null for corrupt stored JSON instead of throwing", () => {
    sessionStorage.setItem("cctrace.pendingRestore", "{not json");
    expect(() => takeRestoreState()).not.toThrow();
    expect(takeRestoreState()).toBeNull();
  });
});

describe("reloadWebview", () => {
  let reloadSpy: ReturnType<typeof vi.fn>;
  const originalLocation = window.location;

  beforeEach(() => {
    vi.useFakeTimers();
    reloadSpy = vi.fn();
    // jsdom's window.location.reload isn't configurable for vi.spyOn directly.
    Object.defineProperty(window, "location", {
      value: { ...originalLocation, reload: reloadSpy },
      writable: true,
      configurable: true,
    });
  });

  afterEach(() => {
    Object.defineProperty(window, "location", {
      value: originalLocation,
      writable: true,
      configurable: true,
    });
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("reloads immediately when nothing is in flight", async () => {
    vi.spyOn(invokeModule, "inFlightInvokeCount").mockReturnValue(0);
    await reloadWebview();
    expect(reloadSpy).toHaveBeenCalled();
  });

  it("waits for in-flight invokes to settle before reloading", async () => {
    // Reloading while a Tauri invoke is pending is a known macOS crash cause
    // (tauri-apps/tauri#9933) — this app always has one in flight, so
    // reloadWebview must wait rather than reload unconditionally.
    let inFlight = 2;
    vi.spyOn(invokeModule, "inFlightInvokeCount").mockImplementation(() => inFlight);

    const done = reloadWebview();
    await vi.advanceTimersByTimeAsync(50);
    expect(reloadSpy).not.toHaveBeenCalled();

    inFlight = 0;
    await vi.advanceTimersByTimeAsync(50);
    await done;
    expect(reloadSpy).toHaveBeenCalled();
  });

  it("reloads anyway after the timeout if invokes never settle", async () => {
    vi.spyOn(invokeModule, "inFlightInvokeCount").mockReturnValue(1);

    const done = reloadWebview();
    await vi.advanceTimersByTimeAsync(3000);
    await done;
    expect(reloadSpy).toHaveBeenCalled();
  });
});
