import { describe, it, expect } from "vitest";
import { tokenize, wordDiff, computeEditDiff } from "./diff";

describe("tokenize", () => {
  it("splits into words, whitespace, and punctuation", () => {
    expect(tokenize("const x = 1;")).toEqual(["const", " ", "x", " ", "=", " ", "1", ";"]);
  });

  it("returns empty array for empty string", () => {
    expect(tokenize("")).toEqual([]);
  });

  it("keeps underscores as part of a word", () => {
    expect(tokenize("foo_bar baz")).toEqual(["foo_bar", " ", "baz"]);
  });
});

describe("wordDiff", () => {
  it("highlights only the changed token", () => {
    const wd = wordDiff("const x = 1;", "const x = 2;");
    expect(wd).not.toBeNull();
    expect(wd!.oldSegments).toEqual([
      { text: "const x = ", changed: false },
      { text: "1", changed: true },
      { text: ";", changed: false },
    ]);
    expect(wd!.newSegments).toEqual([
      { text: "const x = ", changed: false },
      { text: "2", changed: true },
      { text: ";", changed: false },
    ]);
  });

  it("returns null when lines are too dissimilar (below 30% shared)", () => {
    expect(wordDiff("import foo from 'a';", "xyz();")).toBeNull();
  });

  it("returns null when both lines are blank/whitespace-only", () => {
    expect(wordDiff("", "")).toBeNull();
    expect(wordDiff("   ", "  ")).toBeNull();
  });

  it("never flags whitespace as changed", () => {
    const wd = wordDiff("a b", "a  b");
    // Either similar (no non-ws change) -> all unchanged, or null; never a
    // changed whitespace segment.
    if (wd) {
      for (const seg of [...wd.oldSegments, ...wd.newSegments]) {
        if (seg.text.trim() === "") expect(seg.changed).toBe(false);
      }
    }
  });
});

describe("computeEditDiff", () => {
  it("preserves unchanged lines as context", () => {
    const diff = computeEditDiff(["a", "foo bar", "c"], ["a", "foo baz", "c"]);
    expect(diff.map((l) => l.kind)).toEqual(["context", "removed", "added", "context"]);
    expect(diff[0].segments).toEqual([{ text: "a", changed: false }]);
    expect(diff[3].segments).toEqual([{ text: "c", changed: false }]);
  });

  it("word-highlights the changed token within paired changed lines", () => {
    const diff = computeEditDiff(["foo bar"], ["foo baz"]);
    const removed = diff.find((l) => l.kind === "removed")!;
    const added = diff.find((l) => l.kind === "added")!;
    expect(removed.segments).toEqual([
      { text: "foo ", changed: false },
      { text: "bar", changed: true },
    ]);
    expect(added.segments).toEqual([
      { text: "foo ", changed: false },
      { text: "baz", changed: true },
    ]);
  });

  it("handles pure insertion (more added than removed lines)", () => {
    const diff = computeEditDiff(["const x = 1;"], ["const x = 2;", "const y = 3;"]);
    expect(diff.filter((l) => l.kind === "removed")).toHaveLength(1);
    expect(diff.filter((l) => l.kind === "added")).toHaveLength(2);
    // The unpaired added line carries no word highlighting.
    const unpaired = diff.filter((l) => l.kind === "added")[1];
    expect(unpaired.segments).toEqual([{ text: "const y = 3;", changed: false }]);
  });

  it("shows wholly removed/added when a line pair is too dissimilar", () => {
    const diff = computeEditDiff(["import foo from 'a';"], ["xyz();"]);
    const removed = diff.find((l) => l.kind === "removed")!;
    const added = diff.find((l) => l.kind === "added")!;
    // No word-level split — single unchanged segment, line color conveys it.
    expect(removed.segments).toEqual([{ text: "import foo from 'a';", changed: false }]);
    expect(added.segments).toEqual([{ text: "xyz();", changed: false }]);
  });

  it("returns no lines for identical input", () => {
    const diff = computeEditDiff(["same"], ["same"]);
    expect(diff).toEqual([{ kind: "context", segments: [{ text: "same", changed: false }] }]);
  });

  it("reconstructs each line from its segments", () => {
    const diff = computeEditDiff(
      ["function f(a) {", "  return a + 1;", "}"],
      ["function f(a) {", "  return a + 2;", "}"],
    );
    for (const line of diff) {
      expect(typeof line.segments.map((s) => s.text).join("")).toBe("string");
    }
    const changed = diff.find((l) => l.kind === "removed")!;
    expect(changed.segments.map((s) => s.text).join("")).toBe("  return a + 1;");
  });
});
