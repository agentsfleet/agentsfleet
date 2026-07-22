/**
 * Release-gate workflow invariants — the deployment workflow's cache keys,
 * evidence uploads, and notification verdict are release-critical behavior,
 * pinned here against the workflow sources and the extracted verdict script.
 *
 * The workflow YAML assertions are deliberately grep-shaped (exact
 * configuration strings present/absent), mirroring how the release rubric
 * itself audits the file — no YAML tree is reconstructed.
 */
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";

const REPO_ROOT = path.join(__dirname, "../../../..");
const DEPLOY_DEV_WORKFLOW = path.join(REPO_ROOT, ".github/workflows/deploy-dev.yml");
const SMOKE_POST_DEPLOY_WORKFLOW = path.join(REPO_ROOT, ".github/workflows/smoke-post-deploy.yml");
const VERDICT_SCRIPT = path.join(REPO_ROOT, "scripts/dev_release_verdict.sh");

const RAW_RESULTS_DIR = "playwright-acceptance-results";
const DEV_EVIDENCE_ARTIFACT = "acceptance-e2e-dev-results";
const PROD_EVIDENCE_ARTIFACT = "acceptance-e2e-prod-results";
const PHANTOM_LOCK = "ui/packages/app/bun.lock";

const SUCCESS = "success";
const GREEN = "green";
const RED = "red";
const SHORT_SHA = "abc1234";

interface VerdictInputs {
  qa: string;
  acceptance: string;
  cli: string;
  worker: string;
}

interface VerdictOutputs {
  verdict?: string;
  message?: string;
  color?: string;
}

function runVerdict(inputs: VerdictInputs): VerdictOutputs {
  const result = spawnSync("bash", [VERDICT_SCRIPT], {
    env: {
      ...process.env,
      QA_RESULT: inputs.qa,
      ACCEPTANCE_RESULT: inputs.acceptance,
      CLI_RESULT: inputs.cli,
      WORKER_RESULT: inputs.worker,
      REF_NAME: "main",
      SHORT_SHA,
      RUN_URL: "https://example.test/run",
      GITHUB_OUTPUT: "",
    },
    encoding: "utf8",
  });
  expect(result.status, result.stderr).toBe(0);
  const outputs: VerdictOutputs = {};
  for (const line of result.stdout.split("\n")) {
    const eq = line.indexOf("=");
    if (eq > 0) outputs[line.slice(0, eq) as keyof VerdictOutputs] = line.slice(eq + 1);
  }
  return outputs;
}

function deployDevYaml(): string {
  return fs.readFileSync(DEPLOY_DEV_WORKFLOW, "utf8");
}

describe("browser cache and evidence in the deployment workflow", () => {
  it("test_playwright_cache_key_tracks_real_inputs", () => {
    const dev = deployDevYaml();
    const prod = fs.readFileSync(SMOKE_POST_DEPLOY_WORKFLOW, "utf8");
    for (const [label, yaml] of [
      ["deploy-dev", dev],
      ["smoke-post-deploy", prod],
    ] as const) {
      // The phantom per-package lock hashes to nothing and froze the key.
      expect(yaml, `${label} must not hash the phantom lock`).not.toContain(PHANTOM_LOCK);
      // A changed lock or browser version must be an exact miss — partial
      // restores via restore-keys would silently serve a stale browser.
      expect(yaml, `${label} must not soften misses with restore-keys`).not.toContain(
        "restore-keys",
      );
    }
    // Every app-side browser cache keys on the real repo-root lock plus the
    // resolved Playwright version.
    const appCacheKeys = dev.match(/key: .*playwright-app-.*/g) ?? [];
    expect(appCacheKeys.length).toBeGreaterThan(0);
    for (const key of appCacheKeys) {
      expect(key).toContain("hashFiles('bun.lock')");
      expect(key).toContain("outputs.version");
    }
    // The CLI lane keys on its own real lock the same way.
    const cliCacheKeys = dev.match(/key: .*playwright-agentsfleet-.*/g) ?? [];
    expect(cliCacheKeys.length).toBeGreaterThan(0);
    for (const key of cliCacheKeys) {
      expect(key).toContain("hashFiles('cli/bun.lock')");
      expect(key).toContain("outputs.version");
    }
  });

  it("test_acceptance_artifacts_survive_failure", () => {
    const dev = deployDevYaml();
    const prod = fs.readFileSync(SMOKE_POST_DEPLOY_WORKFLOW, "utf8");
    // Raw per-test artifacts are written during the run and uploaded under
    // always(), so failure and cancellation still leave evidence; the
    // rendered report rides along when it exists.
    for (const [label, yaml, artifact] of [
      ["deploy-dev", dev, DEV_EVIDENCE_ARTIFACT],
      ["smoke-post-deploy", prod, PROD_EVIDENCE_ARTIFACT],
    ] as const) {
      expect(yaml, `${label} must name the evidence artifact`).toContain(`name: ${artifact}`);
      expect(yaml, `${label} must upload raw results`).toContain(
        `ui/packages/app/${RAW_RESULTS_DIR}/`,
      );
      expect(yaml, `${label} must upload evidence unconditionally`).toContain("if: always()");
    }
    // The suite writes its raw evidence where the workflow uploads from: the
    // shared output directory plus a machine-readable JSON summary.
    expect(acceptanceConfig.outputDir).toBe(RAW_RESULTS_DIR);
  });
});

describe("the verdict script fails loud and reports every job", () => {
  it("should exit non-zero naming the variable when a job result is missing", () => {
    // A workflow wiring typo (a renamed needs entry) must break the step,
    // never silently default a release-critical result.
    const result = spawnSync("bash", [VERDICT_SCRIPT], {
      env: {
        ...process.env,
        QA_RESULT: SUCCESS,
        ACCEPTANCE_RESULT: SUCCESS,
        WORKER_RESULT: SUCCESS,
        CLI_RESULT: undefined,
      } as NodeJS.ProcessEnv,
      encoding: "utf8",
    });
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("CLI_RESULT");
  });

  it("should emit one summary event per release-critical job", () => {
    // Pins the dev_release_acceptance_summary observability event: one
    // greppable logfmt line per job, carrying only job/result/commit.
    const result = spawnSync("bash", [VERDICT_SCRIPT], {
      env: {
        ...process.env,
        QA_RESULT: SUCCESS,
        ACCEPTANCE_RESULT: SUCCESS,
        CLI_RESULT: SUCCESS,
        WORKER_RESULT: SUCCESS,
        SHORT_SHA,
        GITHUB_OUTPUT: "",
      },
      encoding: "utf8",
    });
    expect(result.status).toBe(0);
    for (const job of ["qa-dev", "acceptance-e2e-dev", "cli-acceptance-dev", "deploy-worker-dev"]) {
      expect(result.stderr).toContain(
        `dev_release_acceptance_summary job=${job} result=${SUCCESS} commit=${SHORT_SHA}`,
      );
    }
  });
});

describe("the notification verdict consumes every gate", () => {
  it("test_dev_notification_includes_cli_result", () => {
    // The workflow feeds the CLI lane's result into the verdict…
    expect(deployDevYaml()).toContain("${{ needs.cli-acceptance-dev.result }}");
    // …and any non-success CLI result is red, including the silent shapes.
    for (const cli of ["failure", "skipped", "cancelled"]) {
      const outputs = runVerdict({ qa: SUCCESS, acceptance: SUCCESS, cli, worker: SUCCESS });
      expect(outputs.verdict, `cli=${cli} must be red`).toBe(RED);
      expect(outputs.message).toContain(`cli-acceptance: ${cli}`);
    }
  });

  it("test_dev_notification_green_requires_all_gates", () => {
    // Only the all-success matrix (worker success or its documented skip) is
    // green, and the verdict reports the release commit.
    for (const worker of [SUCCESS, "skipped"]) {
      const outputs = runVerdict({ qa: SUCCESS, acceptance: SUCCESS, cli: SUCCESS, worker });
      expect(outputs.verdict, `worker=${worker} all-success must be green`).toBe(GREEN);
      expect(outputs.message).toContain(SHORT_SHA);
    }
    const degraded: VerdictInputs[] = [
      { qa: "failure", acceptance: SUCCESS, cli: SUCCESS, worker: SUCCESS },
      { qa: SUCCESS, acceptance: "failure", cli: SUCCESS, worker: SUCCESS },
      { qa: SUCCESS, acceptance: "cancelled", cli: SUCCESS, worker: SUCCESS },
      { qa: SUCCESS, acceptance: "skipped", cli: SUCCESS, worker: SUCCESS },
      { qa: SUCCESS, acceptance: SUCCESS, cli: "failure", worker: SUCCESS },
      { qa: SUCCESS, acceptance: SUCCESS, cli: SUCCESS, worker: "failure" },
      { qa: SUCCESS, acceptance: SUCCESS, cli: SUCCESS, worker: "cancelled" },
    ];
    for (const inputs of degraded) {
      const outputs = runVerdict(inputs);
      expect(outputs.verdict, JSON.stringify(inputs)).toBe(RED);
    }
  });
});
