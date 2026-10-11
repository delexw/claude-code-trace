import { describe, it, expect, beforeEach } from "vitest";
import { loadBrowseMode, saveBrowseMode, BROWSE_MODE_KEY } from "./browseMode";

describe("browseMode", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to all when nothing is stored", () => {
    expect(loadBrowseMode()).toBe("all");
  });

  it("round-trips each mode", () => {
    for (const mode of ["claude", "antigravity", "all"] as const) {
      saveBrowseMode(mode);
      expect(loadBrowseMode()).toBe(mode);
    }
  });

  it("falls back to the default for an unrecognised stored value", () => {
    // A value from a newer build (or a hand-edited one) must not put the
    // picker into a mode it has no handling for.
    localStorage.setItem(BROWSE_MODE_KEY, "gemini");
    expect(loadBrowseMode()).toBe("all");
  });

  it("uses the documented storage key", () => {
    saveBrowseMode("antigravity");
    expect(localStorage.getItem(BROWSE_MODE_KEY)).toBe("antigravity");
  });
});
