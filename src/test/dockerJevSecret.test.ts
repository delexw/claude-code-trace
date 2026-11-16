import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const repoRoot = resolve(__dirname, "../..");
const redeployScript = join(repoRoot, "script/redeploy.sh");
const entrypointScript = join(repoRoot, "script/docker-entrypoint.sh");

function runRedeployFunction(source: string, input?: string) {
  return spawnSync("bash", ["-c", `source "$REDEPLOY_SCRIPT"; ${source}`], {
    env: { ...process.env, REDEPLOY_SCRIPT: redeployScript },
    input,
    encoding: "utf8",
  });
}

function listContainersPublishingPort(port: string, dockerBody: string) {
  return runRedeployFunction(`
    docker() {
      ${dockerBody}
    }
    container_ids_publishing_port ${port}
  `);
}

describe("Docker redeploy port cleanup", () => {
  it("finds containers publishing the port without the engine-specific publish filter", () => {
    const result = listContainersPublishingPort(
      "1421",
      `
        printf '%s\\n' "$*" >&2
        printf '%s\\n' \\
          'aaa111 0.0.0.0:1421->1421/tcp, [::]:1421->1421/tcp' \\
          'bbb222 0.0.0.0:11421->80/tcp' \\
          'ccc333 0.0.0.0:8080->1421/tcp' \\
          'ddd444 '
      `,
    );

    expect(result.status).toBe(0);
    expect(result.stdout).toBe("aaa111\n");
    expect(result.stderr).not.toContain("--filter");
  });

  it("returns nothing when no container publishes the port", () => {
    const result = listContainersPublishingPort("1421", "printf '%s\\n' 'ddd444 '");

    expect(result.status).toBe(0);
    expect(result.stdout).toBe("");
  });

  it("fails loudly when the engine rejects the listing", () => {
    const result = listContainersPublishingPort(
      "1421",
      `
        echo 'Error response from daemon: boom' >&2
        return 1
      `,
    );

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("Error response from daemon: boom");
  });
});

describe("Docker Jev API-key setup", () => {
  it.each(["y", "Y", "yes", "YES"])("accepts %s as consent to configure", (answer) => {
    const result = runRedeployFunction("ask_to_configure_jev_api_key", `${answer}\n`);

    expect(result.status).toBe(0);
  });

  it.each(["", "n", "no"])("treats %s as no configuration", (answer) => {
    const result = runRedeployFunction("ask_to_configure_jev_api_key", `${answer}\n`);

    expect(result.status).not.toBe(0);
  });

  it("reads the Jev key without echoing it to stderr", () => {
    const result = runRedeployFunction("read_jev_api_key", "hidden-jev-secret\n");

    expect(result.status).toBe(0);
    expect(result.stdout).toBe("hidden-jev-secret");
    expect(result.stderr).not.toContain("hidden-jev-secret");
  });

  it("rejects an empty Jev key", () => {
    const result = runRedeployFunction("read_jev_api_key", "   \n");

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("Jev API key cannot be empty");
  });

  it("passes the hidden key through stdin rather than Docker arguments", () => {
    const scratch = mkdtempSync(join(tmpdir(), "cctrace-docker-secret-"));
    const callsFile = join(scratch, "docker-calls");
    const secretInputFile = join(scratch, "secret-input");
    const source = `
      docker() {
        printf '%s\\n' "$*" >> "$DOCKER_CALLS_FILE"
        cat > "$SECRET_INPUT_FILE"
      }
      store_docker_jev_api_key "test-jev-secret"
    `;

    const result = spawnSync("bash", ["-c", `source "$REDEPLOY_SCRIPT"; ${source}`], {
      env: {
        ...process.env,
        REDEPLOY_SCRIPT: redeployScript,
        DOCKER_CALLS_FILE: callsFile,
        SECRET_INPUT_FILE: secretInputFile,
      },
      encoding: "utf8",
    });

    expect(result.status).toBe(0);
    expect(readFileSync(secretInputFile, "utf8")).toBe("test-jev-secret");
    expect(readFileSync(callsFile, "utf8")).not.toContain("test-jev-secret");
    expect(readFileSync(callsFile, "utf8")).toContain("compose run");
  });

  it("loads the mounted secret before starting the headless server", () => {
    const scratch = mkdtempSync(join(tmpdir(), "cctrace-entrypoint-secret-"));
    const secretFile = join(scratch, "jev_api_key");
    writeFileSync(secretFile, "mounted-jev-secret\n", { mode: 0o600 });

    const output = execFileSync(
      entrypointScript,
      ["sh", "-c", 'printf "%s" "$JEV_API_KEY"', "--headless"],
      {
        env: { ...process.env, JEV_API_KEY_FILE: secretFile, JEV_API_KEY: "" },
        encoding: "utf8",
      },
    );

    expect(output).toBe("mounted-jev-secret");
  });

  it("keeps an explicitly supplied environment key authoritative", () => {
    const scratch = mkdtempSync(join(tmpdir(), "cctrace-entrypoint-env-"));
    const secretFile = join(scratch, "jev_api_key");
    writeFileSync(secretFile, "mounted-jev-secret\n", { mode: 0o600 });

    const output = execFileSync(
      entrypointScript,
      ["sh", "-c", 'printf "%s" "$JEV_API_KEY"', "--headless"],
      {
        env: {
          ...process.env,
          JEV_API_KEY_FILE: secretFile,
          JEV_API_KEY: "environment-jev-secret",
        },
        encoding: "utf8",
      },
    );

    expect(output).toBe("environment-jev-secret");
  });

  it("keeps the Jev key in the single config volume", () => {
    const compose = readFileSync(join(repoRoot, "docker-compose.yml"), "utf8");

    expect(compose).toContain("JEV_API_KEY_FILE: /home/app/.config/claude-code-trace/jev_api_key");
    expect(compose).toContain("cctrace-config:/home/app/.config");
    expect(compose.match(/^ {2}\S+:$/gm)).toEqual(["  cctrace:", "  cctrace-config:"]);
  });
});
