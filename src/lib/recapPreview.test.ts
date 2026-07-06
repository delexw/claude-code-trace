import { describe, it, expect, beforeEach, vi } from "vitest";
import { RECAP_PREVIEW_KEY, loadRecapPreview, saveRecapPreview } from "./recapPreview";

describe("recapPreview", () => {
  beforeEach(() => localStorage.clear());
  it("defaults to true when unset", () => {
    expect(loadRecapPreview()).toBe(true);
  });
  it("round-trips false", () => {
    saveRecapPreview(false);
    expect(localStorage.getItem(RECAP_PREVIEW_KEY)).toBe("false");
    expect(loadRecapPreview()).toBe(false);
  });
  it("ignores write failures", () => {
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => saveRecapPreview(false)).not.toThrow();
    spy.mockRestore();
  });
});
