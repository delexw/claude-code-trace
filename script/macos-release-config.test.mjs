import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

describe("macOS release config", () => {
  it("ad-hoc signs the app bundle", () => {
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(config.bundle.macOS.signingIdentity).toBe("-");
  });

  it("builds both the app tarball the installer fetches and a dmg", () => {
    const workflow = read(".github/workflows/release.yml");
    const macosJob = workflow.slice(
      workflow.indexOf("build-macos:"),
      workflow.indexOf("build-linux:"),
    );
    expect(macosJob).toMatch(/args: --bundles app,dmg\n/);
    expect(read("script/install-macos.sh")).toContain("Claude.Code.Trace_aarch64.app.tar.gz");
  });
});
