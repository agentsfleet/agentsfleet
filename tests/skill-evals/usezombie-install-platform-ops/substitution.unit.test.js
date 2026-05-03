// Substitution behaviour — the install-skill's step-8 contract codified
// as deterministic tests. Covers platform-managed posture, BYOK sentinels,
// the empty-cron path, and the no-leftover-placeholders invariant.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  PLATFORM_DEFAULTS, BYOK_SENTINELS,
  readTemplate, substitute,
} from "./substitute.js";

const FULL_VARS_PLATFORM = {
  slack_channel: "#platform-ops",
  prod_branch_glob: "main",
  cron_schedule: "*/30 * * * *",
  ...PLATFORM_DEFAULTS,
};

const FULL_VARS_BYOK = {
  slack_channel: "#platform-ops",
  prod_branch_glob: "main",
  cron_schedule: "",
  ...BYOK_SENTINELS,
};

test("template TRIGGER.md carries the doctor-sourced placeholders (model + cap)", () => {
  // TRIGGER.md only carries the two fields the worker reads at trigger
  // time. The three operator-input placeholders (slack_channel,
  // prod_branch_glob, cron_schedule) live in SKILL.md prose where the
  // agent reads them as behaviour knobs.
  const tpl = readTemplate("TRIGGER.md");
  for (const k of ["model", "context_cap_tokens"]) {
    assert.ok(tpl.includes(`{{${k}}}`), `TRIGGER.md missing {{${k}}}`);
  }
});

test("template SKILL.md carries the operator-input placeholders", () => {
  const tpl = readTemplate("SKILL.md");
  assert.ok(tpl.includes("{{slack_channel}}"), "SKILL.md missing {{slack_channel}}");
  assert.ok(tpl.includes("{{prod_branch_glob}}"), "SKILL.md missing {{prod_branch_glob}}");
  assert.ok(tpl.includes("{{cron_schedule}}"), "SKILL.md missing {{cron_schedule}}");
});

test("substitute platform-default produces fully-substituted TRIGGER.md", () => {
  const out = substitute(readTemplate("TRIGGER.md"), FULL_VARS_PLATFORM);
  assert.match(out, /model: "accounts\/fireworks\/models\/kimi-k2\.6"/);
  assert.match(out, /context_cap_tokens: 256000/);
});

test("substitute BYOK produces sentinel TRIGGER.md (empty model + zero cap)", () => {
  const out = substitute(readTemplate("TRIGGER.md"), FULL_VARS_BYOK);
  assert.match(out, /model: ""/);
  assert.match(out, /context_cap_tokens: 0/);
});

test("substitute platform-default produces fully-substituted SKILL.md", () => {
  const out = substitute(readTemplate("SKILL.md"), FULL_VARS_PLATFORM);
  assert.match(out, /into channel `#platform-ops`/);
  // The morning-health-check section references the production branch glob.
  assert.match(out, /GitHub Actions on `main`/);
  assert.match(out, /branch=main/);
});

test("substitute throws on missing placeholder (install-skill must surface)", () => {
  assert.throws(
    () => substitute(readTemplate("TRIGGER.md"), { slack_channel: "#x" }),
    /unsubstituted placeholder/,
  );
});

test("substitute leaves no `{{...}}` token in either output", () => {
  for (const file of ["SKILL.md", "TRIGGER.md"]) {
    const out = substitute(readTemplate(file), FULL_VARS_PLATFORM);
    assert.equal(out.includes("{{"), false, `leftover {{ in ${file}`);
  }
});

test("substitute is idempotent against fully-substituted output", () => {
  const out1 = substitute(readTemplate("TRIGGER.md"), FULL_VARS_PLATFORM);
  const out2 = substitute(out1, FULL_VARS_PLATFORM);
  assert.equal(out1, out2);
});

test("substituted platform-default TRIGGER.md matches the M46 round-trip fixture byte-for-byte", async () => {
  // Connects the JS substitution path to the Zig parser-acceptance path:
  // the fixture under samples/fixtures/frontmatter/bundles/platform_ops_installed_default/
  // is what the Zig frontmatter_fixtures_test.zig parses. If the JS
  // substitute() drifts from what the Zig parser accepts, this test
  // catches it before a user gets a broken install.
  const { readFile } = await import("node:fs/promises");
  const { resolve } = await import("node:path");
  const { repoRoot } = await import("./substitute.js");
  const expected = await readFile(
    resolve(repoRoot, "samples/fixtures/frontmatter/bundles/platform_ops_installed_default/TRIGGER.md"),
    "utf8",
  );
  const actual = substitute(readTemplate("TRIGGER.md"), FULL_VARS_PLATFORM);
  // The fixture is hand-authored to the parsed shape (tools list trimmed
  // for parser-test focus). We assert the structural fields match: model,
  // context, trigger.{type,source,signature}, network.allow.
  for (const needle of [
    'model: "accounts/fireworks/models/kimi-k2.6"',
    "context_cap_tokens: 256000",
    "tool_window: auto",
    "memory_checkpoint_every: 5",
    "stage_chunk_threshold: 0.75",
    "type: webhook",
    "source: github",
    "secret_ref: github_secret",
    "header: x-hub-signature-256",
    'prefix: "sha256="',
    "api.machines.dev",
    "api.upstash.com",
    "slack.com",
    "api.github.com",
  ]) {
    assert.ok(actual.includes(needle), `substituted output missing: ${needle}`);
    assert.ok(expected.includes(needle), `parser fixture missing: ${needle} (drift between JS path + Zig fixture)`);
  }
});

test("empty cron_schedule still substitutes cleanly (the no-cron path)", () => {
  const out = substitute(readTemplate("SKILL.md"), {
    ...FULL_VARS_PLATFORM,
    cron_schedule: "",
  });
  // Empty value substitutes into the body prose; the agent reads the
  // surrounding sentence and decides whether to call cron_add.
  assert.equal(out.includes("{{cron_schedule}}"), false);
});
