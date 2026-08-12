/**
 * Standing contracts over the acceptance tree, enforced from the unit lane
 * (vitest excludes tests/e2e/** as test FILES, so the greps must live here):
 *
 *  - the shared TRIGGER.md fixture carries every frontmatter key the daemon's
 *    importer requires (name, triggers, tools, budget — see
 *    fleet_runtime/config_parser.zig);
 *  - no spec re-grows a private bundle builder — the drifted copies are how
 *    installs started failing with UZ-BUNDLE-001;
 *  - no test waits on `networkidle`, which is unreachable while the Clerk
 *    testing proxy holds retried FAPI requests in-flight.
 */
import { readdirSync, readFileSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";
import { triggerMd } from "./e2e/acceptance/fixtures/seed";

const TESTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const ACCEPTANCE_DIR = path.join(TESTS_DIR, "e2e", "acceptance");
const SHARED_FIXTURES_SUFFIX = path.join("fixtures", "seed.ts");
const LOAD_STATE_NO_TEST_MAY_AWAIT = "networkidle";

function acceptanceSources(): string[] {
  return readdirSync(ACCEPTANCE_DIR, { recursive: true, encoding: "utf8" })
    .filter((rel) => rel.endsWith(".ts"))
    .map((rel) => path.join(ACCEPTANCE_DIR, rel));
}

test("test_shared_trigger_fixture_satisfies_required_keys", () => {
  const md = triggerMd("hygiene-check");
  expect(md.startsWith("---\n")).toBe(true);
  expect(md).toContain("name: hygiene-check");
  expect(md).toContain("triggers:");
  expect(md).toContain("tools:");
  expect(md).toContain("budget:");
});

test("test_no_spec_local_bundle_builders_remain", () => {
  const offenders = acceptanceSources()
    .filter((file) => !file.endsWith(SHARED_FIXTURES_SUFFIX))
    .filter((file) =>
      /function (triggerMd|skillMd|emptyBodySkillMd)/.test(readFileSync(file, "utf8")),
    );
  expect(offenders).toEqual([]);
});

test("test_no_networkidle_in_acceptance_suite", () => {
  const offenders = acceptanceSources().filter((file) =>
    readFileSync(file, "utf8").includes(LOAD_STATE_NO_TEST_MAY_AWAIT),
  );
  expect(offenders).toEqual([]);
});
