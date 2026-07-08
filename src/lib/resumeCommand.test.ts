import { describe, it, expect } from "vitest";
import { resumeCommand } from "./resumeCommand";

describe("resumeCommand", () => {
  it("builds a quoted cd + resume", () => {
    expect(resumeCommand("/a b/c", "uuid-1")).toBe("cd '/a b/c' && claude --resume uuid-1");
  });
  it("adds --fork-session when fork", () => {
    expect(resumeCommand("/x", "u2", { fork: true })).toBe(
      "cd '/x' && claude --resume u2 --fork-session",
    );
  });
  it("escapes single quotes in the path", () => {
    expect(resumeCommand("/it's/here", "u3")).toBe("cd '/it'\\''s/here' && claude --resume u3");
  });
});
