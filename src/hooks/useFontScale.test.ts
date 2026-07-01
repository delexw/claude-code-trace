import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useFontScale } from "./useFontScale";
import { FONT_SCALE_KEY } from "../lib/fontScale";

describe("useFontScale", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.zoom = "";
  });

  it("defaults to 100% and applies it to the document", () => {
    const { result } = renderHook(() => useFontScale());
    expect(result.current[0]).toBe(1);
    expect(document.documentElement.style.zoom).toBe("1");
  });

  it("initialises from a persisted value", () => {
    localStorage.setItem(FONT_SCALE_KEY, "1.5");
    const { result } = renderHook(() => useFontScale());
    expect(result.current[0]).toBe(1.5);
    expect(document.documentElement.style.zoom).toBe("1.5");
  });

  it("updates, persists, and re-applies on change", () => {
    const { result } = renderHook(() => useFontScale());
    act(() => result.current[1](1.25));
    expect(result.current[0]).toBe(1.25);
    expect(localStorage.getItem(FONT_SCALE_KEY)).toBe("1.25");
    expect(document.documentElement.style.zoom).toBe("1.25");
  });

  it("clamps out-of-range updates", () => {
    const { result } = renderHook(() => useFontScale());
    act(() => result.current[1](99));
    expect(result.current[0]).toBe(2);
  });
});
