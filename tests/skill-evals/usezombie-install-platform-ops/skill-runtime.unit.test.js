// Runtime-behavior pin tests for the install-skill body. SKILL.md is
// prose that a host LLM (Claude Code / Amp / Codex CLI / OpenCode)
// interprets to execute the install plan — there is no Node.js
// runtime executing the steps, so "runtime behavior" tests have to
// pin the canonical prose the LLM reads at each decision point. A
// drift in any of these strings = a drift in what the LLM does at
// that step. Sister to skill-body.unit.test.js (shape) and
// substitution.unit.test.js (the one Node-side helper).
//
// Each test below maps to one spec test-row described in the
// install-skill spec's Test Specification table. Spec test ID is in
// each test's docstring (the `test_skill_s*` snake-case names).
//
// What these tests do NOT cover: end-to-end execution of the skill via
// a real LLM with mocked gh — that's llm-judge.eval.js territory and a
// genuinely different cost surface (real API calls). Pin tests are the
// floor; LLM-judge eval is the optional ceiling.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..", "..", "..");
const skillBody = readFileSync(
  resolve(repoRoot, "skills", "usezombie-install-platform-ops", "SKILL.md"),
  "utf8",
);
const triggerTemplate = readFileSync(
  resolve(repoRoot, "samples", "platform-ops", "TRIGGER.md"),
  "utf8",
);

// Spec row: test_skill_s1_0_precondition_passes
// Mock environment with zombiectl + gh + auth → S1.0 advances.
// The LLM reads the canonical one-liner and runs it; the prose must
// state both the check command and the "advance on success" contract
// for the LLM to follow.
test("s1.0 — precondition check command + stop-on-miss prose", () => {
  // The check itself: three binaries + doctor probe in one line. This
  // is what the LLM types verbatim per the prose.
  assert.match(
    skillBody,
    /which zombiectl && which gh && zombiectl doctor --json/,
    "SKILL.md S1.0 lost the precondition one-liner",
  );
  // Stop-on-miss contract — the LLM only advances on success.
  assert.match(
    skillBody,
    /Any miss → print the exact one-liner above to fix it and stop\./,
    "SKILL.md S1.0 lost the stop-on-miss contract",
  );
});

// Spec row: test_skill_s1_0_missing_zombiectl
// Skill prints `npm install -g @usezombie/zombiectl`, stops.
// The remediation prose for a missing zombiectl is the install command
// itself — the "Any miss → print the exact one-liner above" rule
// points the LLM at the npm install line in the preceding code block.
test("s1.0 — missing-zombiectl remediation: npm install one-liner", () => {
  // The exact remediation string.
  assert.match(
    skillBody,
    /npm install -g @usezombie\/zombiectl/,
    "SKILL.md S1.0 lost the npm-install remediation",
  );
  // The remediation must precede the "any miss → print and stop" rule
  // so the LLM resolves "the exact one-liner above" correctly. Index
  // check: remediation line first, then the rule.
  const installIdx = skillBody.indexOf("npm install -g @usezombie/zombiectl");
  const stopIdx = skillBody.indexOf("Any miss → print the exact one-liner above");
  assert.ok(installIdx > -1 && stopIdx > -1 && installIdx < stopIdx,
    "remediation must precede the stop-on-miss rule (LLM resolves 'one-liner above')");
});

// Spec row: test_skill_s1_0_missing_gh_scope
// Skill prints `gh auth refresh -s admin:repo_hook`, stops.
// Two surfaces: the precondition block (cold-machine auth) and the
// step-9 failure-mode (mid-run scope loss). Both must teach the same
// refresh command verbatim — drift = LLM tells the user one thing on
// cold install and another mid-flight.
test("s1.0 — missing-gh-scope remediation: gh auth refresh one-liner", () => {
  // Cold-install surface in the precondition block.
  assert.match(
    skillBody,
    /gh auth login -s admin:repo_hook/,
    "SKILL.md S1.0 lost the cold-install gh scope grant",
  );
  // Step-9 mid-run recovery surface — same scope, refresh verb.
  assert.match(
    skillBody,
    /gh auth refresh -s admin:repo_hook/,
    "SKILL.md S1.9 lost the mid-run gh scope refresh",
  );
});

// Spec row: test_skill_s1_8_parses_triggers
// Rendered TRIGGER.md with triggers: [github, cron] → skill captures
// both, loops S1.9 only on webhook trigger.
// Two pins: (a) the SKILL.md S1.8 prose tells the LLM to extract
// `x-usezombie.triggers[]` from rendered TRIGGER.md and skip non-webhook
// entries; (b) the TRIGGER.md template carries the `triggers:` array
// shape so the LLM has something to parse.
test("s1.8 — parses triggers[] from rendered TRIGGER.md, skips non-webhook for S1.9", () => {
  // S1.8 prose names the array path + skip-non-webhook rule.
  assert.match(
    skillBody,
    /extract\s+`x-usezombie\.triggers\[\]`/,
    "SKILL.md S1.8 lost the triggers[] extraction prose",
  );
  // `\s+` not literal space — the prose wraps `entries\n   (cron / api)`.
  assert.match(
    skillBody,
    /Skip non-webhook entries\s+\(cron \/ api\)/,
    "SKILL.md S1.8 lost the skip-non-webhook contract",
  );
  // TRIGGER.md template carries the triggers[] array shape so an
  // LLM following S1.8 has a parseable target.
  assert.ok(
    /x-usezombie:[\s\S]*triggers:/m.test(triggerTemplate),
    "TRIGGER.md template missing x-usezombie.triggers: array shape",
  );
  // The template's first trigger entry must be a webhook + source pair
  // so a default install gives the LLM at least one S1.9 iteration.
  assert.match(
    triggerTemplate,
    /-\s+type:\s*webhook/,
    "TRIGGER.md template missing a webhook trigger entry",
  );
  assert.match(
    triggerTemplate,
    /source:\s*github/,
    "TRIGGER.md template missing a github source — default install has no S1.9 work to do",
  );
});

// Spec row: test_skill_s1_9_gh_api_invocation
// Mock gh records command; assert matches template with substituted
// URL, events, secret reference.
// We pin every load-bearing field of the gh-api command template so a
// regression in the prose (dropped --field, renamed key) fails here
// rather than at first user install.
test("s1.9 — gh api invocation template carries every load-bearing field", () => {
  // Verb + endpoint
  assert.match(
    skillBody,
    /gh api -X POST "repos\/\$\{GH_REPO\}\/hooks"/,
    "S1.9 gh api template missing POST verb or /hooks endpoint",
  );
  // Required fields — name=web (GitHub's hook-type discriminator),
  // active=true (so the hook fires), events[] (the array shape that
  // the EVENTS substitution feeds), config.url + content_type +
  // secret.
  for (const field of [
    "--field name=web",
    "--field active=true",
    "events[]",
    'config[url]=${WEBHOOK_URL}',
    "config[content_type]=json",
    "config[secret]=${WEBHOOK_SECRET}",
  ]) {
    assert.ok(
      skillBody.includes(field),
      `S1.9 gh api template missing field: ${field}`,
    );
  }
});

// Spec row: test_skill_s1_9_422_idempotent
// Mock gh returns 422 hook-exists → skill GETs hooks, matches URL,
// advances.
// Pin: the prose teaches the LLM that 422 is idempotent (re-running
// the install must not error out on the second pass) and the recovery
// path is a GET on the hooks endpoint with URL-match.
test("s1.9 — 422 hook-exists is idempotent: GET hooks, match URL, advance", () => {
  assert.match(
    skillBody,
    /422 Hook already exists/,
    "S1.9 lost the 422 idempotency rule",
  );
  assert.match(
    skillBody,
    /idempotent:\s*`gh api\s*[\r\n]?\s*repos\/\$\{GH_REPO\}\/hooks`\s*\(GET\)/,
    "S1.9 422 recovery must say GET hooks, then match URL",
  );
  assert.match(
    skillBody,
    /matches\s+`\$\{WEBHOOK_URL\}`/,
    "S1.9 422 recovery must match by webhook URL (idempotency key)",
  );
});

// Spec row: test_skill_s1_9_403_scope_recovery
// Mock gh returns 403 → skill prints refresh command, stops.
// Pin: 403/401 case explicitly listed as "missing scope" and the
// recovery is the exact gh-auth-refresh one-liner from S1.0's
// remediation surface. Drift here = LLM tells user something
// different at install vs at runtime — confusing.
test("s1.9 — 403/401 = missing scope; print refresh one-liner and stop", () => {
  assert.match(
    skillBody,
    /`403`\s*\/\s*`401`\s*\(missing scope\)/,
    "S1.9 403/401 row lost the 'missing scope' label",
  );
  // The recovery command must be the canonical gh-auth-refresh
  // one-liner (also pinned in test_skill_s1_0_missing_gh_scope).
  assert.match(
    skillBody,
    /gh auth refresh -s admin:repo_hook/,
    "S1.9 403 recovery missing refresh one-liner",
  );
  // Hard-stop rule — don't silently retry, surface the failure.
  assert.match(
    skillBody,
    /Never silently\s*retry a failed step/,
    "skill body lost the no-silent-retry contract",
  );
});

// Spec row: test_skill_s1_10_hmac_self_verify
// Skill computes HMAC over canned payload, curls receiver, asserts 202.
// Pin every byte of the self-verify block: the HMAC algorithm name,
// the openssl invocation, the X-Hub-Signature-256 header shape, the
// curl POST, the expected 202 status, and the stop-on-mismatch rule.
test("s1.10 — HMAC self-verify block: openssl + curl + 202 + stop-on-mismatch", () => {
  // Algorithm + secret source
  assert.match(
    skillBody,
    /compute HMAC-SHA256 of a synthetic payload/,
    "S1.10 lost the HMAC-SHA256 algorithm name",
  );
  // openssl invocation — the canonical command bytes the LLM types
  assert.match(
    skillBody,
    /openssl dgst -sha256 -hmac "\$WEBHOOK_SECRET"/,
    "S1.10 lost the openssl HMAC invocation",
  );
  // X-Hub-Signature-256 header — the GitHub-canonical webhook signature
  // header. Drift here = receiver rejects the self-verify.
  assert.match(
    skillBody,
    /X-Hub-Signature-256:\s*sha256=\$\{SIG\}/,
    "S1.10 lost the X-Hub-Signature-256 header line",
  );
  // curl POST to the receiver URL
  assert.match(
    skillBody,
    /curl -fsS -X POST "\$\{WEBHOOK_URL\}"/,
    "S1.10 lost the curl POST line",
  );
  // Expected 202 — anything else stops the install
  assert.match(
    skillBody,
    /Expect HTTP\s+`202`/,
    "S1.10 lost the 'expect 202' contract",
  );
  // Stop on non-202 / HMAC mismatch (no silent advance)
  assert.match(
    skillBody,
    /On non-202, network failure, or\s*HMAC mismatch, print the response verbatim and stop/,
    "S1.10 lost the stop-on-mismatch rule",
  );
});
