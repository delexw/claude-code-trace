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

  it("produces the exact PowerShell command shared with the terminal UI", () => {
    expect(
      codexSessionCommand({
        id,
        cwd: "D:\\Toy Projects\\it's here",
        path: "C:\\Users\\demo\\custom codex\\sessions\\rollout-fabricated.jsonl",
      })?.command,
    ).toBe(
      `& { $codexTracePreviousHome = $env:CODEX_HOME; try { $env:CODEX_HOME = 'C:\\Users\\demo\\custom codex'; codex -C 'D:\\Toy Projects\\it''s here' resume -- '${id}' } finally { $env:CODEX_HOME = $codexTracePreviousHome } }`,
    );
  });

  // PowerShell closes a single-quoted string at any of these, not only at '.
  const powerShellQuotes = ["'", "\u2018", "\u2019", "\u201A", "\u201B"];

  it.each(powerShellQuotes)(
    "doubles the PowerShell quote %s so a crafted value cannot close the string",
    (quote) => {
      const generated = codexSessionCommand({
        id: `${quote}; Write-Host id ${quote}`,
        cwd: `D:\\a${quote}; Write-Host cwd ${quote}b`,
        path: `C:\\h${quote}; Write-Host home ${quote}\\sessions\\rollout-fabricated.jsonl`,
      });
      const doubled = quote + quote;
      expect(generated?.shell).toBe("PowerShell");
      expect(generated?.command).toBe(
        `& { $codexTracePreviousHome = $env:CODEX_HOME; try { $env:CODEX_HOME = 'C:\\h${doubled}; Write-Host home ${doubled}'; ` +
          `codex -C 'D:\\a${doubled}; Write-Host cwd ${doubled}b' resume -- '${doubled}; Write-Host id ${doubled}' ` +
          `} finally { $env:CODEX_HOME = $codexTracePreviousHome } }`,
      );
    },
  );

  it("doubles every quote in a run of mixed PowerShell quotes", () => {
    const generated = codexSessionCommand({
      id,
      cwd: "D:\\a'\u2018\u2019\u201A\u201Bz",
      path: "C:\\h\\sessions\\rollout-fabricated.jsonl",
    });
    expect(generated?.command).toContain(
      "codex -C 'D:\\a''\u2018\u2018\u2019\u2019\u201A\u201A\u201B\u201Bz' resume",
    );
  });

  it("leaves curly quotes alone in POSIX commands, where they are ordinary text", () => {
    const generated = codexSessionCommand({
      id,
      cwd: "/work/it\u2019s",
      path: "/custom/codex/sessions/rollout-fabricated.jsonl",
    });
    expect(generated?.command).toBe(
      `CODEX_HOME='/custom/codex' codex -C '/work/it\u2019s' resume -- '${id}'`,
    );
  });

  it("does not map a /mnt path that contains a line separator onto a drive", () => {
    // JavaScript's `.` stops at U+2028 and U+2029; the terminal UI must agree.
    expect(codexSessionCommand({ ...session, cwd: "/mnt/c/a\u2028b" })?.command).toBe(
      `CODEX_HOME='/custom/codex' codex -C '/mnt/c/a\u2028b' resume -- '${id}'`,
    );
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
    ...["\0", "\r", "\n"].flatMap((control) => [
      { ...session, id: `abc${control}def` },
      { ...session, cwd: `/work/a${control}b` },
      { ...session, path: `/custom/codex/sessions/rollout-a${control}b.jsonl` },
    ]),
  ])("does not build a command from an unusable location: %j", (value) => {
    expect(codexSessionCommand(value)).toBeNull();
  });
});
