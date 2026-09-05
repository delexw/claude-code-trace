import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * `vite.config.ts` is loaded (and therefore its imports resolved) before the
 * Docker frontend build runs, but nothing links the config's import list to
 * the `COPY` lines in the Dockerfile. Add an import there and forget the
 * `COPY` here and the image build fails with UNRESOLVED_IMPORT — but only in
 * Docker, never locally. This test closes that gap.
 */

const repoRoot = resolve(__dirname, "../..");

/** Repo-relative paths that `vite.config.ts` imports from the local tree. */
function localImportsOfViteConfig(): string[] {
  const source = readFileSync(join(repoRoot, "vite.config.ts"), "utf8");
  const specifiers = [...source.matchAll(/from\s+"(\.[^"]*)"/g)].map((m) => m[1]);
  return specifiers.map((spec) => spec.replace(/^\.\//, ""));
}

/** Build-context paths copied into the `frontend-builder` stage. */
function frontendBuilderCopySources(): string[] {
  const dockerfile = readFileSync(join(repoRoot, "Dockerfile"), "utf8");
  const stage = dockerfile.split(/^FROM .*AS frontend-builder$/m)[1]?.split(/^FROM /m)[0];
  expect(stage, "Dockerfile has no frontend-builder stage").toBeDefined();

  return (stage as string)
    .split("\n")
    .filter((line) => line.startsWith("COPY "))
    .flatMap((line) => {
      const args = line.slice("COPY ".length).split(/\s+/).filter(Boolean);
      // Drop `--from=…`-style flags and the trailing destination argument.
      return args.filter((arg) => !arg.startsWith("--")).slice(0, -1);
    });
}

describe("Docker frontend-builder build context", () => {
  it("copies every local file vite.config.ts imports", () => {
    const copied = frontendBuilderCopySources();

    for (const imported of localImportsOfViteConfig()) {
      const covered = copied.some(
        (source) => source === imported || imported.startsWith(`${source}/`),
      );
      expect(covered, `Dockerfile frontend-builder stage never COPYs "${imported}"`).toBe(true);
    }
  });

  it("copies vite.config.ts itself", () => {
    expect(frontendBuilderCopySources()).toContain("vite.config.ts");
  });
});
