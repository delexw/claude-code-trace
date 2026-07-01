import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  FONT_SCALE_KEY,
  DEFAULT_FONT_SCALE,
  MIN_FONT_SCALE,
  MAX_FONT_SCALE,
  clampFontScale,
  readStoredFontScale,
  storeFontScale,
  applyFontScale,
  formatFontScale,
} from "./fontScale";

describe("fontScale", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.zoom = "";
  });

  describe("clampFontScale", () => {
    it("keeps in-range values", () => {
      expect(clampFontScale(1.25)).toBe(1.25);
    });

    it("clamps below the minimum and above the maximum", () => {
      expect(clampFontScale(0.1)).toBe(MIN_FONT_SCALE);
      expect(clampFontScale(99)).toBe(MAX_FONT_SCALE);
    });

    it("falls back to the default for non-finite values", () => {
      expect(clampFontScale(Number.NaN)).toBe(DEFAULT_FONT_SCALE);
      expect(clampFontScale(Number.POSITIVE_INFINITY)).toBe(DEFAULT_FONT_SCALE);
    });
  });

  describe("readStoredFontScale", () => {
    it("returns the default when unset", () => {
      expect(readStoredFontScale()).toBe(DEFAULT_FONT_SCALE);
    });

    it("reads and clamps a stored value", () => {
      localStorage.setItem(FONT_SCALE_KEY, "1.5");
      expect(readStoredFontScale()).toBe(1.5);
      localStorage.setItem(FONT_SCALE_KEY, "5");
      expect(readStoredFontScale()).toBe(MAX_FONT_SCALE);
    });

    it("returns the default for malformed values", () => {
      localStorage.setItem(FONT_SCALE_KEY, "not-a-number");
      expect(readStoredFontScale()).toBe(DEFAULT_FONT_SCALE);
    });
  });

  describe("storeFontScale", () => {
    it("round-trips through readStoredFontScale", () => {
      storeFontScale(1.75);
      expect(readStoredFontScale()).toBe(1.75);
    });

    it("ignores write failures", () => {
      const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
        throw new Error("quota");
      });
      expect(() => storeFontScale(1.25)).not.toThrow();
      spy.mockRestore();
    });
  });

  describe("applyFontScale", () => {
    it("sets the document zoom, clamping out-of-range values", () => {
      applyFontScale(1.25);
      expect(document.documentElement.style.zoom).toBe("1.25");
      applyFontScale(10);
      expect(document.documentElement.style.zoom).toBe(String(MAX_FONT_SCALE));
    });
  });

  describe("formatFontScale", () => {
    it("renders a percentage label", () => {
      expect(formatFontScale(1)).toBe("100%");
      expect(formatFontScale(1.25)).toBe("125%");
      expect(formatFontScale(0.8)).toBe("80%");
    });
  });
});
