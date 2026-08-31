/**
 * Release-gate workflow invariants — the deployment pipeline's cache keys,
 * evidence uploads, and notification verdict are release-critical behavior,
 * pinned here against the workflow sources and the extracted verdict script.
 *
 * The development pipeline spans a job graph and called stage workflows.
 * Its Bun and Playwright setup, including the cache key, lives in the shared
 * composite action. Each invariant is asserted against the file that owns it,
 * and family-wide bans scan every workflow file.
 *
 * The workflow YAML assertions are deliberately grep-shaped (exact
 * configuration strings present/absent), mirroring how the release rubric
 * itself audits the files — no YAML tree is reconstructed.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import acceptanceConfig from "../playwright.acceptance.config";

const REPO_ROOT = path.join(__dirname, "../../../..");
const WORKFLOWS_DIR = path.join(REPO_ROOT, ".github/workflows");
const DEPLOY_DEV_WORKFLOW = path.join(WORKFLOWS_DIR, "deploy-dev.yml");
const POST_RELEASE_WORKFLOW = path.join(WORKFLOWS_DIR, "post-release.yml");
const RELEASE_WORKFLOW = path.join(WORKFLOWS_DIR, "release.yml");
const SMOKE_POST_DEPLOY_WORKFLOW = path.join(WORKFLOWS_DIR, "smoke-post-deploy.yml");
const PLAYWRIGHT_SETUP_ACTION = path.join(
  REPO_ROOT,
  ".github/actions/setup-bun-playwright/action.yml",
);

const RAW_RESULTS_DIR = "playwright-acceptance-results";
const DEV_EVIDENCE_ARTIFACT = "acceptance-e2e-results";
const PROD_EVIDENCE_ARTIFACT = "acceptance-e2e-prod-results";
const PHANTOM_LOCK = "ui/packages/app/bun.lock";

function deployDevYaml(): string {
  return fs.readFileSync(DEPLOY_DEV_WORKFLOW, "utf8");
}

function postReleaseYaml(): string {
  return fs.readFileSync(POST_RELEASE_WORKFLOW, "utf8");
}

function releaseYaml(): string {
  return fs.readFileSync(RELEASE_WORKFLOW, "utf8");
}

/** Every file of the dev pipeline (caller + called stages), concatenated. */
function deployDevFamily(): string {
  return fs
    .readdirSync(WORKFLOWS_DIR)
    .filter((f) => f.startsWith("deploy-dev") && f.endsWith(".yml"))
    .sort()
    .map((f) => fs.readFileSync(path.join(WORKFLOWS_DIR, f), "utf8"))
    .join("\n");
}

function playwrightSetupAction(): string {
  return fs.readFileSync(PLAYWRIGHT_SETUP_ACTION, "utf8");
}

describe("browser cache and evidence in the deployment workflow", () => {
  it("test_playwright_cache_key_tracks_real_inputs", () => {
    const devFamily = deployDevFamily();
    const setupAction = playwrightSetupAction();
    const prod = fs.readFileSync(SMOKE_POST_DEPLOY_WORKFLOW, "utf8");
    for (const [label, yaml] of [
      ["deploy-dev family", devFamily],
      ["setup-bun-playwright", setupAction],
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
    // The one parameterized cache key lives in the composite action and keys
    // on the real inputs: the caller-supplied lockfile hash plus the resolved
    // Playwright version.
    const actionKeys = setupAction.match(/key: .*\n/g) ?? [];
    expect(actionKeys.length).toBeGreaterThan(0);
    for (const key of actionKeys) {
      expect(key).toContain("hashFiles(inputs.lock-file)");
      expect(key).toContain("outputs.version");
    }
    // Every app-side job feeds the composite the real repo-root lock (the
    // workspace has ONE lock; ui/packages/app has none of its own)…
    expect(devFamily).toMatch(
      /cache-prefix: playwright-app\n\s+[^\n]*\n?\s*lock-file: bun\.lock/,
    );
    // …and the CLI lane keys on its own real lock the same way.
    expect(devFamily).toMatch(
      /cache-prefix: playwright-agentsfleet\n[\s\S]{0,200}?lock-file: cli\/bun\.lock/,
    );
    // No job bypasses the composite with a hand-rolled browser cache.
    expect(devFamily, "dev pipeline must not inline a Playwright cache key").not.toMatch(
      /key: .*playwright-(app|agentsfleet)-/,
    );
  });

  it("test_acceptance_artifacts_survive_failure", () => {
    const devFamily = deployDevFamily();
    const prod = fs.readFileSync(SMOKE_POST_DEPLOY_WORKFLOW, "utf8");
    // Raw per-test artifacts are written during the run and uploaded under
    // always(), so failure and cancellation still leave evidence; the
    // rendered report rides along when it exists.
    for (const [label, yaml, artifact] of [
      ["deploy-dev family", devFamily, DEV_EVIDENCE_ARTIFACT],
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

describe("artifacts uploaded are the artifacts downloaded", () => {
  it("should download every artifact the build jobs upload", () => {
    // The bug this pins: the jobs were renamed and their artifacts with them
    // (`dev-binaries`/`dev-daemon` → `runner-binary`/`daemon-binary`), but the
    // download glob stayed `dev-*`. Nothing fails at rename time — the pattern
    // is still valid, it just matches nothing — so `push-ghcr` would have
    // downloaded zero binaries and the image COPY would have died on the first
    // merge to main. A stale glob is silent in a way a stale job name is not:
    // a bad `needs:` is a parse error, a bad `pattern:` is an empty directory.
    const workflow = deployDevFamily();
    const uploaded = [...workflow.matchAll(/^\s+name: ([a-z0-9-]+-binary)$/gm)]
      .map((m) => m[1])
      .filter((v): v is string => v !== undefined);
    expect(uploaded.length).toBeGreaterThan(0);

    const patterns = [...workflow.matchAll(/^\s+pattern: '?([^'\n]+)'?$/gm)]
      .map((m) => m[1])
      .filter((v): v is string => v !== undefined)
      .map((v) => v.trim());
    expect(patterns.length).toBeGreaterThan(0);

    for (const artifact of uploaded) {
      const matched = patterns.some((p) => {
        const re = new RegExp(`^${p.split("*").map((x) => x.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*")}$`);
        return re.test(artifact);
      });
      expect(matched, `no download pattern matches uploaded artifact '${artifact}'`).toBe(true);
    }
  });
});

describe("the release verdict reports every job", () => {
  it("should emit one summary event per release-critical job", () => {
    const workflow = deployDevYaml();
    for (const job of [
      "compile-runner",
      "compile-daemon",
      "push-ghcr",
      "deploy-fly",
      "qa",
      "acceptance-e2e",
      "acceptance-cli",
      "deploy-metal",
    ]) {
      expect(workflow).toContain(`"${job}=$`);
    }
    expect(workflow).toContain("dev_release_acceptance_summary job=${entry%%=*}");
  });

  it("should name the stage that broke, not report a build failure as four bare skips", () => {
    // The bug: build and Fly had no line in the verdict, so a failed image push
    // rendered as `QA: skipped | acceptance-e2e: skipped | acceptance-cli:
    // skipped | metal: skipped` — red, correctly, with nothing saying why. The
    // reader had to open the run to learn whether the push failed, Fly refused,
    // or /readyz never came up.
    const workflow = deployDevYaml();
    expect(workflow).toContain("RUNNER_BUILD: ${{ needs.build.outputs.runner }}");
    expect(workflow).toContain("DAEMON_BUILD: ${{ needs.build.outputs.daemon }}");
    expect(workflow).toContain("GHCR_RESULT: ${{ needs.build.outputs.ghcr }}");
    expect(workflow).toContain("FLY_RESULT: ${{ needs.fly.outputs.result }}");
    expect(workflow).toContain(
      "build: runner ${RUNNER_BUILD} | daemon ${DAEMON_BUILD} | ghcr ${GHCR_RESULT} | fly ${FLY_RESULT}",
    );
    // notify must depend on the stages it reports, or the outputs are empty.
    expect(workflow).toContain("needs: [build, fly, acceptance, metal]");
  });

  it("should default every reported stage to skipped so an empty output never reads as a pass", () => {
    // A called workflow that never ran returns "" for its outputs. Without the
    // :-skipped default an unset stage is neither success nor skipped, and a
    // string comparison against "success" is the only thing standing between
    // that and a green verdict on a deploy that did not happen.
    const workflow = deployDevYaml();
    for (const v of [
      "RUNNER_BUILD",
      "DAEMON_BUILD",
      "GHCR_RESULT",
      "FLY_RESULT",
      "QA_RESULT",
      "ACCEPTANCE_RESULT",
      "CLI_RESULT",
      "METAL_RESULT",
    ]) {
      expect(workflow).toContain(`${v}="\${${v}:-skipped}"`);
    }
  });

  it("should report build and fly without re-judging them in the green condition", () => {
    // Deliberate restraint, pinned so nobody "completes" it later: nothing
    // downstream can pass if the image never pushed, so the acceptance outputs
    // come back empty and the verdict is already red on their account. Adding
    // build and fly to the green condition would be redundant logic whose only
    // possible contribution is a new way to be wrong.
    const workflow = deployDevYaml();
    // The WHOLE condition, not the tail after an anchor: a stage added BEFORE
    // the anchor would slip past a split-and-inspect-the-rest assertion. (It
    // did — this test was written that way first and a mutant survived it.)
    const condition = workflow.slice(workflow.indexOf("\n          if [ "), workflow.indexOf("; then"));
    expect(condition).toContain("QA_RESULT");
    for (const reported of ["RUNNER_BUILD", "DAEMON_BUILD", "GHCR_RESULT", "FLY_RESULT"]) {
      expect(condition).not.toContain(reported);
    }
  });
});

describe("the notification verdict consumes every gate", () => {
  it("test_dev_notification_includes_cli_result", () => {
    // Gate results cross the reusable-workflow boundary as outputs — a called
    // workflow's own result collapses to one bit, which would hide WHICH gate
    // broke the release. The verdict must read the granular output, and the
    // acceptance workflow must actually export it from the job result.
    const workflow = deployDevYaml();
    expect(workflow).toContain("CLI_RESULT: ${{ needs.acceptance.outputs.cli }}");
    expect(workflow).toContain('[ "$CLI_RESULT" = success ]');
    expect(workflow).toContain("acceptance-cli: ${CLI_RESULT}");
    const family = deployDevFamily();
    expect(family).toContain("cli: ${{ needs.acceptance-cli.result }}");
  });

  it("test_dev_notification_green_requires_all_gates", () => {
    const workflow = deployDevYaml();
    expect(workflow).toContain('[ "$QA_RESULT" = success ]');
    expect(workflow).toContain('[ "$ACCEPTANCE_RESULT" = success ]');
    expect(workflow).toContain('[ "$CLI_RESULT" = success ]');
    expect(workflow).toContain('[ "$METAL_RESULT" = success ] || [ "$METAL_RESULT" = skipped ]');
    expect(workflow).toContain("✅ DEV deploy green");
    expect(workflow).toContain("❌ DEV deploy not releasable");
    // An upstream failure that skipped a whole stage leaves its output empty;
    // the verdict must read that as skipped — red — never as a pass.
    expect(workflow).toContain('QA_RESULT="${QA_RESULT:-skipped}"');
    expect(workflow).toContain('METAL_RESULT="${METAL_RESULT:-skipped}"');
  });
});

describe("post-release promotion follows exact-version acceptance", () => {
  it("pins installation and acceptance to the triggering release", () => {
    const workflow = postReleaseYaml();
    expect(workflow).toContain("ref: ${{ github.event.workflow_run.head_sha }}");
    expect(workflow).toContain('test "$(npm view "@agentsfleet/cli@$VERSION" version)" = "$VERSION"');
    expect(workflow).toContain(
      "npm install -g @agentsfleet/cli@${{ needs.resolve-release.outputs.version }}",
    );
    expect(workflow).not.toContain("npm install -g @agentsfleet/cli@latest");
  });

  it("blocks latest promotion behind successful production acceptance", () => {
    const workflow = postReleaseYaml();
    expect(workflow).toContain("if: vars.PROD_RUNNER_READY == 'true'");
    expect(workflow).toContain("needs: [resolve-release, verify-npm, acceptance-cli-prod]");
    expect(workflow).toContain('npm dist-tag add "@agentsfleet/cli@$VERSION" latest');

    const promotion = workflow.split("  promote-latest:")[1]?.split("\n  summary:")[0];
    expect(promotion).toBeDefined();
    expect(promotion).not.toContain("if: always()");
  });
});

describe("deployment workflows keep mutable values out of shell source", () => {
  it("passes repository variables through environment maps", () => {
    const development = deployDevFamily();
    const production = releaseYaml();

    expect(development).toContain("VAULT_DEV: ${{ vars.VAULT_DEV }}");
    expect(development).not.toContain('VAULT_DEV="${{ vars.VAULT_DEV }}"');
    for (const variable of ["FLY_APP_PROD", "VAULT_PROD"]) {
      expect(production).toContain(`${variable}: \${{ vars.${variable} }}`);
      expect(production).not.toContain(`${variable}="\${{ vars.${variable} }}"`);
    }
    expect(production).toContain("RUNNER_ITEM: ${{ steps.canary.outputs.vault_key }}");
    expect(production).not.toContain('RUNNER_ITEM="${{ steps.canary.outputs.vault_key }}"');
  });
});

describe("production runner rollout is canary-first and fail-closed", () => {
  it("validates the fleet inventory before selecting any host", () => {
    const workflow = releaseYaml();
    expect(workflow).toContain('select(type == "array" and length > 0)');
    expect(workflow).toContain('test("^[A-Za-z0-9][A-Za-z0-9._-]*$")');
    expect(workflow).toContain("| .[0].vault_key");
    expect(workflow).toContain("all(.[].vault_key;");
    expect(workflow).toContain("jq -r '.[1:][] | .vault_key'");
  });

  it("deploys and verifies the canary before the approved fleet rollout", () => {
    const workflow = releaseYaml();
    const canary = workflow
      .split("  deploy-metal-canary-prod:")[1]
      ?.split("\n  deploy-metal-fleet-prod:")[0];
    const fleet = workflow.split("  deploy-metal-fleet-prod:")[1];

    expect(canary).toBeDefined();
    expect(canary).toContain("./playbooks/lib/runner/deploy.sh");
    expect(canary).toContain("./playbooks/lib/runner/verify.sh");
    expect(fleet).toBeDefined();
    expect(fleet).toContain("needs: deploy-metal-canary-prod");
    expect(fleet).toContain("environment: production-fleet");
    expect(fleet).toContain('for runner in "${runners[@]}"; do');
    expect(fleet).toContain("./playbooks/lib/runner/deploy.sh");
    expect(fleet).toContain("./playbooks/lib/runner/verify.sh");
  });
});
