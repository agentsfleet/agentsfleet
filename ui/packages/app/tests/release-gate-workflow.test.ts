/**
 * Release-gate workflow invariants — the deployment workflow's cache keys,
 * evidence uploads, and notification verdict are release-critical behavior,
 * pinned here against the workflow sources and the extracted verdict script.
 *
 * The workflow YAML assertions are deliberately grep-shaped (exact
 * configuration strings present/absent), mirroring how the release rubric
 * itself audits the file — no YAML tree is reconstructed.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";

const REPO_ROOT = path.join(__dirname, "../../../..");
const DEPLOY_DEV_WORKFLOW = path.join(REPO_ROOT, ".github/workflows/deploy-dev.yml");
const SMOKE_POST_DEPLOY_WORKFLOW = path.join(REPO_ROOT, ".github/workflows/smoke-post-deploy.yml");

const RAW_RESULTS_DIR = "playwright-acceptance-results";
const DEV_EVIDENCE_ARTIFACT = "acceptance-e2e-dev-results";
const PROD_EVIDENCE_ARTIFACT = "acceptance-e2e-prod-results";
const PHANTOM_LOCK = "ui/packages/app/bun.lock";

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

describe("the release verdict reports every job", () => {
  it("should emit one summary event per release-critical job", () => {
    const workflow = deployDevYaml();
    for (const job of ["qa-dev", "acceptance-e2e-dev", "cli-acceptance-dev", "deploy-worker-dev"]) {
      expect(workflow).toContain(`"${job}=$`);
    }
    expect(workflow).toContain("dev_release_acceptance_summary job=${entry%%=*}");
  });
});

describe("the notification verdict consumes every gate", () => {
  it("test_dev_notification_includes_cli_result", () => {
    const workflow = deployDevYaml();
    expect(workflow).toContain("CLI_RESULT: ${{ needs.cli-acceptance-dev.result }}");
    expect(workflow).toContain('[ "$CLI_RESULT" = success ]');
    expect(workflow).toContain("cli-acceptance: ${CLI_RESULT}");
  });

  it("test_dev_notification_green_requires_all_gates", () => {
    const workflow = deployDevYaml();
    expect(workflow).toContain('[ "$QA_RESULT" = success ]');
    expect(workflow).toContain('[ "$ACCEPTANCE_RESULT" = success ]');
    expect(workflow).toContain('[ "$CLI_RESULT" = success ]');
    expect(workflow).toContain('[ "$WORKER_RESULT" = success ] || [ "$WORKER_RESULT" = skipped ]');
    expect(workflow).toContain("✅ DEV deploy green");
    expect(workflow).toContain("❌ DEV deploy not releasable");
  });
});
