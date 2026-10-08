/**
 * Release verdict invariants — the dev pipeline's notify job turns every
 * stage's result into one green-or-red message, and that message is what a
 * release decision is made from. Pinned against `.github/workflows/deploy-dev.yml`,
 * which owns the verdict, and the called stage workflows that export the
 * results it reads (`deploy-dev-build.yml` declares the build stage's outputs).
 *
 * Grep-shaped like `release-gate-workflow.test.ts`: exact configuration
 * strings present or absent, no YAML tree reconstructed.
 */
import { describe, expect, it } from "vitest";
import { deployDevFamily, deployDevYaml } from "./helpers/release-workflows";

// Stages the verdict REPORTS but does not re-judge: a broken build or Fly
// deploy leaves the acceptance outputs empty, which is already red.
const REPORTED_STAGES = [
  "RUNNER_BUILD",
  "TOOLBOX_BUILD",
  "DAEMON_BUILD",
  "GHCR_RESULT",
  "FLY_RESULT",
] as const;
// Gates whose own result decides the verdict.
const GATE_RESULTS = ["QA_RESULT", "ACCEPTANCE_RESULT", "CLI_RESULT", "METAL_RESULT"] as const;

describe("the release verdict reports every job", () => {
  it("should emit one summary event per release-critical job", () => {
    const workflow = deployDevYaml();
    for (const job of [
      "compile-runner",
      "build-toolbox",
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
    expect(workflow).toContain("TOOLBOX_BUILD: ${{ needs.build.outputs.toolbox }}");
    expect(workflow).toContain("DAEMON_BUILD: ${{ needs.build.outputs.daemon }}");
    expect(workflow).toContain("GHCR_RESULT: ${{ needs.build.outputs.ghcr }}");
    expect(workflow).toContain("FLY_RESULT: ${{ needs.fly.outputs.result }}");
    expect(workflow).toContain(
      "build: runner ${RUNNER_BUILD} | toolbox ${TOOLBOX_BUILD} | daemon ${DAEMON_BUILD} | ghcr ${GHCR_RESULT} | fly ${FLY_RESULT}",
    );
    // The build stage must export the toolbox result, or the line above always
    // reads `toolbox skipped` whatever the build-toolbox job did.
    expect(deployDevFamily()).toContain("toolbox: ${{ needs.build-toolbox.result }}");
    // notify must depend on the stages it reports, or the outputs are empty.
    expect(workflow).toContain("needs: [build, fly, acceptance, metal]");
  });

  it("should default every reported stage to skipped so an empty output never reads as a pass", () => {
    // A called workflow that never ran returns "" for its outputs. Without the
    // :-skipped default an unset stage is neither success nor skipped, and a
    // string comparison against "success" is the only thing standing between
    // that and a green verdict on a deploy that did not happen.
    const workflow = deployDevYaml();
    for (const v of [...REPORTED_STAGES, ...GATE_RESULTS]) {
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
    for (const reported of REPORTED_STAGES) {
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
