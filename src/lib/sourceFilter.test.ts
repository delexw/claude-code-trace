import { describe, it, expect, beforeEach } from "vitest";
import { ALL_SOURCES, SOURCE_FILTER_KEY, loadSourceFilter, saveSourceFilter } from "./sourceFilter";

describe("sourceFilter", () => {
  beforeEach(() => localStorage.clear());

  it("shows every source when nothing is stored", () => {
    expect(loadSourceFilter()).toBe(ALL_SOURCES);
  });

  it("remembers the chosen source", () => {
    saveSourceFilter("antigravity");
    expect(localStorage.getItem(SOURCE_FILTER_KEY)).toBe("antigravity");
    expect(loadSourceFilter()).toBe("antigravity");
  });
});
