#!/usr/bin/env node
/**
 * One-shot setup for the Playwright suite, run by the first `webServer` entry
 * in playwright.config.ts before the backend starts:
 *
 * 1. Recreate `e2e/.tmp` with an isolated config dir + a copy of the session
 *    fixtures for each deployment shape (tests mutate both).
 * 2. Build the headless backend binary (no Tauri/GTK). Set
 *    `CCTRACE_E2E_SKIP_BUILD=1` to reuse an existing build.
 * 3. Build the frontend bundle the same way the Docker image does
 *    (`VITE_API_BASE=""` → same-origin relative URLs) into `e2e/.tmp/dist`.
 */
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { snapshotRealSecrets } from "./real-secrets.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tmp = resolve(root, "e2e/.tmp");
const fixtures = resolve(root, "e2e/fixtures/projects");

function seedEfficiencyAnalysis(configDir, projectsDir) {
  const sessionPath = resolve(projectsDir, "-tmp-e2e-demo/e2e-session.jsonl");
  const contents = readFileSync(sessionPath);
  const cacheName = `${createHash("sha256").update(sessionPath).digest("base64url")}.json`;
  const analysisDir = resolve(configDir, "analysis");
  mkdirSync(analysisDir, { recursive: true });
  writeFileSync(
    resolve(analysisDir, cacheName),
    JSON.stringify(
      {
        sessionId: "e2e-session",
        sessionPath,
        score: 82,
        dimensions: {
          progress: 80,
          toolUse: 70,
          focus: 60,
          exploration: 50,
          recovery: 40,
          tokenUse: 30,
        },
        metricEvaluations: [
          {
            key: "progressingEfficiently",
            label: "Progress",
            question: "Did the agent make steady, meaningful progress toward the user's task?",
            higherProbabilityIsBetter: true,
            probability: 0.8,
            score: 80,
          },
          {
            key: "toolCallsUseful",
            label: "Useful tool calls",
            question: "Were the tool calls useful and proportionate to completing the task?",
            higherProbabilityIsBetter: true,
            probability: 0.7,
            score: 70,
          },
          {
            key: "redundantWorkPresent",
            label: "Avoided repeated work",
            question: "Was materially redundant or repeated work present?",
            higherProbabilityIsBetter: false,
            probability: 0.2,
            score: 80,
          },
          {
            key: "excessiveExploration",
            label: "Proportionate exploration",
            question: "Was exploration excessive relative to the task?",
            higherProbabilityIsBetter: false,
            probability: 0.3,
            score: 70,
          },
          {
            key: "likelyThrashing",
            label: "Avoided thrashing",
            question: "Did the agent cycle among similar actions without meaningful progress?",
            higherProbabilityIsBetter: false,
            probability: 0.1,
            score: 90,
          },
          {
            key: "effectiveRecovery",
            label: "Effective recovery",
            question: "When mistakes or failures occurred, did the agent recover effectively?",
            higherProbabilityIsBetter: true,
            probability: 0.6,
            score: 60,
          },
          {
            key: "tokenUsageEfficient",
            label: "Efficient token use",
            question: "Was token usage efficient for the work completed?",
            higherProbabilityIsBetter: true,
            probability: 0.7,
            score: 70,
          },
          {
            key: "subagentsUseful",
            label: "Useful subagents",
            question: "If subagents were used, did they add useful independent work?",
            higherProbabilityIsBetter: true,
            probability: 0.5,
            score: 50,
          },
          {
            key: "likelyTaskCompleted",
            label: "Task completion",
            question:
              "Does the trace indicate that the user's requested task was completed successfully?",
            higherProbabilityIsBetter: true,
            probability: 0.8,
            score: 80,
          },
        ],
        findings: [],
        decisions: {
          progressingEfficiently: 0.8,
          toolCallsUseful: 0.7,
          redundantWorkPresent: 0.2,
          excessiveExploration: 0.3,
          likelyThrashing: 0.1,
          effectiveRecovery: 0.6,
          tokenUsageEfficient: 0.7,
          subagentsUseful: 0.5,
          likelyTaskCompleted: 0.8,
        },
        analyzedAt: "2026-09-19T04:00:00.000Z",
        analyzedTurns: 3,
        transcriptFingerprint: createHash("sha256").update(contents).digest("base64url"),
        analysisVersion: 5,
        decisionSetVersion: 3,
        scoreFormulaVersion: 2,
        stale: false,
      },
      null,
      2,
    ),
  );
}

rmSync(tmp, { recursive: true, force: true });
mkdirSync(tmp, { recursive: true });
// Record the state of every secret in the *real* config dir before any server
// starts, so specs can prove the run neither wrote nor deleted one there (see
// "keeps every secret on the test path" in same-origin.spec.ts).
writeFileSync(resolve(tmp, "real-secrets.json"), JSON.stringify(snapshotRealSecrets(), null, 2));
for (const shape of ["same", "web"]) {
  const configDir = resolve(tmp, shape, "config");
  const projectsDir = resolve(tmp, shape, "projects");
  mkdirSync(configDir, { recursive: true });
  cpSync(fixtures, projectsDir, { recursive: true });
  seedEfficiencyAnalysis(configDir, projectsDir);
}

const run = (cmd, env = {}) =>
  execSync(cmd, { cwd: root, stdio: "inherit", env: { ...process.env, ...env } });

if (!process.env.CCTRACE_E2E_SKIP_BUILD) {
  run(
    "cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --bin claude-code-trace",
  );
}
run(`npx vite build --outDir "${resolve(tmp, "dist")}" --emptyOutDir`, { VITE_API_BASE: "" });

console.log(`e2e: scratch space ready at ${tmp}`);
