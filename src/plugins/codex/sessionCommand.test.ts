import { describe, expect, it } from "vitest";
import { codexSessionCommand } from "./sessionCommand";

const id = "00000000-0000-0000-0000-000000000123";
const session = {
  id,
  cwd: "/work/toy calculator",
  path: "/custom/codex/sessions/2026/01/01/rollout-fabricated.jsonl",
};

describe("Codex session commands", () => {
  it("resumes the selected session in its original folder and custom home", () => {
    expect(codexSessionCommand(session)).toEqual({
      shell: "POSIX shell",
      command: `CODEX_HOME='/custom/codex' codex -C '/work/toy calculator' resume -- '${id}'`,
    });
  });

  it("forks archived sessions with the fork subcommand", () => {
    expect(
      codexSessionCommand(
        { ...session, path: "/custom/codex/archived_sessions/rollout-fabricated.jsonl" },
        { fork: true },
      )?.command,
    ).toBe(`CODEX_HOME='/custom/codex' codex -C '/work/toy calculator' fork -- '${id}'`);
  });

  it("keeps shell metacharacters literal in POSIX paths and IDs", () => {
    const generated = codexSessionCommand({
      cwd: "/work/it's $(printf injected); `whoami`",
      id: "--last'; printf injected #",
      path: "/it's/$(whoami)/sessions/rollout-fabricated.jsonl",
    });
    expect(generated?.command).toBe(
      "CODEX_HOME='/it'\\''s/$(whoami)' codex -C '/work/it'\\''s $(printf injected); `whoami`' resume -- '--last'\\''; printf injected #'",
    );
  });

  it("uses PowerShell quoting for native Windows data", () => {
    const generated = codexSessionCommand({
      id,
      cwd: "D:\\Toy Projects\\it's $safe`",
      path: "C:\\Users\\demo\\custom codex\\sessions\\rollout-fabricated.jsonl",
    });
    expect(generated?.shell).toBe("PowerShell");
    expect(generated?.command).toContain("$env:CODEX_HOME = 'C:\\Users\\demo\\custom codex'");
    expect(generated?.command).toContain(
      "codex -C 'D:\\Toy Projects\\it''s $safe`' resume -- '" + id + "'",
    );
    expect(generated?.command).toContain("finally { $env:CODEX_HOME = $codexTracePreviousHome }");
  });

  it("converts a Windows home read through WSL back to its native path", () => {
    const generated = codexSessionCommand({
      id,
      cwd: "D:\\Toy Projects\\calculator",
      path: "/mnt/c/Users/demo/.codex/sessions/rollout-fabricated.jsonl",
    });
    expect(generated?.shell).toBe("PowerShell");
    expect(generated?.command).toContain("$env:CODEX_HOME = 'C:\\Users\\demo\\.codex'");
    expect(generated?.command).not.toContain("/mnt/c");
  });

  it.each(["\\\\wsl.localhost\\ExampleLinux", "\\\\wsl$\\ExampleLinux"])(
    "uses a native POSIX home for sessions discovered through %s",
    (prefix) => {
      const generated = codexSessionCommand({
        ...session,
        path: `${prefix}\\home\\demo\\.codex\\sessions\\rollout-fabricated.jsonl`,
      });
      expect(generated?.shell).toBe("POSIX shell");
      expect(generated?.command).toContain("CODEX_HOME='/home/demo/.codex'");
    },
  );

  it.each([
    { ...session, cwd: "" },
    { ...session, id: "" },
    { ...session, path: "/unrecognized/rollout.jsonl" },
    { ...session, cwd: "relative" },
    { ...session, cwd: "D:\\toy", path: "/unmapped/codex/sessions/rollout.jsonl" },
    { ...session, id: "first\nsecond" },
  ])("does not build a command from an unusable location: %j", (value) => {
    expect(codexSessionCommand(value)).toBeNull();
  });
});
