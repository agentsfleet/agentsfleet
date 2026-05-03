// Repo-detection sanity checks — the install-skill's step-3 contract.
// The skill body teaches the agent which files to look for; these
// fixtures exist so an LLM-judge test (nightly) can run the skill body
// against them and verify the agent correctly classifies. Per-PR we
// just assert the fixtures are well-formed — wrong fixture data would
// silently neuter every other test that depends on them.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixtures = resolve(__dirname, "..", "fixtures");

test("fixture gh-actions-fly: workflow + fly.toml + package.json present", () => {
  const root = resolve(fixtures, "gh-actions-fly");
  assert.ok(existsSync(resolve(root, ".github/workflows/deploy.yml")));
  assert.ok(existsSync(resolve(root, "fly.toml")));
  assert.ok(existsSync(resolve(root, "package.json")));
});

test("fixture gh-actions-only: workflow present, no fly.toml", () => {
  const root = resolve(fixtures, "gh-actions-only");
  assert.ok(existsSync(resolve(root, ".github/workflows/test.yml")));
  assert.equal(existsSync(resolve(root, "fly.toml")), false);
});

test("fixture no-ci: nothing the skill can install against", () => {
  const root = resolve(fixtures, "no-ci");
  assert.equal(existsSync(resolve(root, ".github")), false);
  assert.equal(existsSync(resolve(root, "fly.toml")), false);
  // package.json present so the dir isn't accidentally empty — but the
  // skill's step-3 contract is "no .github/workflows/ → stop".
  assert.ok(existsSync(resolve(root, "package.json")));
});
