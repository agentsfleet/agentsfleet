// Invariant tests on the agent skill body itself — Resend-pattern
// frontmatter shape, host-neutrality, references resolve, file-length
// hygiene. None of these need a running LLM; they assert the skill body
// is the shape the runtime + every supported host expect.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..", "..", "..");
const skillDir = resolve(repoRoot, "skills", "usezombie-install-platform-ops");
const skillBody = readFileSync(resolve(skillDir, "SKILL.md"), "utf8");

function frontmatter(md) {
  const m = md.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) throw new Error("no frontmatter");
  return m[1];
}

test("Resend-pattern frontmatter — required keys present", () => {
  const fm = frontmatter(skillBody);
  for (const key of ["name:", "description:", "license:", "metadata:", "inputs:", "references:"]) {
    assert.ok(fm.includes(key), `frontmatter missing ${key}`);
  }
});

test("frontmatter declares all three operator inputs", () => {
  const fm = frontmatter(skillBody);
  for (const input of ["slack_channel", "prod_branch_glob", "cron_schedule"]) {
    assert.ok(fm.includes(`name: ${input}`), `inputs missing ${input}`);
  }
});

test("frontmatter declares required + optional binaries", () => {
  const fm = frontmatter(skillBody);
  assert.match(fm, /bins: \[zombiectl, openssl, curl\]/);
  assert.match(fm, /optional_bins: \[op\]/);
});

test("body does not hard-code any one host's question primitive", () => {
  // Skill must work in Claude Code, Amp, Codex CLI, OpenCode. Hard-coding
  // AskUserQuestion (Claude-specific) as the only resolution path breaks
  // Amp/Codex/OpenCode users. Two controlled mentions are allowed:
  // (a) the Authentication-section explanatory aside ("e.g. Claude Code's
  // `AskUserQuestion`") and (b) the Common Mistakes anti-pattern row.
  // Anywhere else is a regression.
  const offenders = ["AskUserQuestion", "ClaudeAskUser"];
  for (const off of offenders) {
    const occurrences = skillBody.split(off).length - 1;
    assert.ok(occurrences <= 2, `body references ${off} ${occurrences}× — at most 2× (explanatory aside + negated anti-pattern row)`);
  }
});

test("every reference in frontmatter resolves to an existing file", () => {
  const fm = frontmatter(skillBody);
  const refs = fm.match(/references\/[a-z-]+\.md/g) ?? [];
  assert.ok(refs.length >= 3, `expected ≥3 references, got ${refs.length}`);
  for (const r of refs) {
    assert.ok(existsSync(resolve(skillDir, r)), `reference missing on disk: ${r}`);
  }
});

test("body uses canonical CLI verbs (add/show/delete) — no stale set/get", () => {
  // RULE NLG: pre-v2.0.0 spec carries no legacy verb framing. Verbs
  // landed in commit 30025b34; spec aligned in 1912bba7; skill body
  // must not regress.
  assert.equal(skillBody.includes("credential set"), false, "skill body uses stale `credential set`");
  assert.equal(skillBody.includes("credential get"), false, "skill body uses stale `credential get`");
  assert.equal(skillBody.includes("credential remove"), false, "skill body uses stale `credential remove`");
});

test("body teaches `--data @-` not `--data '<JSON>'`", () => {
  assert.match(skillBody, /credential add <name> --data @-/);
  // The unsafe form is allowed up to twice in the body — the
  // Authentication-section "never pass JSON via `--data '<JSON>'`"
  // sentence and the Common Mistakes anti-pattern row. Anywhere else
  // is a regression toward writing it as the recommended form.
  const unsafeCount = (skillBody.match(/--data ['"]<JSON>['"]/g) ?? []).length;
  assert.ok(unsafeCount <= 2, `body references unsafe --data '<JSON>' form ${unsafeCount}× — at most 2× (negated anti-pattern mentions)`);
});

test("body covers every step in the install plan (1-12)", () => {
  for (let i = 1; i <= 12; i++) {
    assert.ok(skillBody.includes(`${i}.`), `body missing step ${i}.`);
  }
});

test("body length stays under 350 lines (RULE FLL)", () => {
  const lines = skillBody.split("\n").length;
  assert.ok(lines <= 350, `SKILL.md is ${lines} lines (cap 350)`);
});

test("references docs cover the three contracts", () => {
  const credRes = readFileSync(resolve(skillDir, "references/credential-resolution.md"), "utf8");
  for (const cred of ["github", "fly", "slack", "upstash"]) {
    assert.ok(credRes.includes(cred), `credential-resolution.md missing ${cred}`);
  }
  const failModes = readFileSync(resolve(skillDir, "references/failure-modes.md"), "utf8");
  // Every numbered step the skill body documents should have a row.
  for (const step of ["1 — doctor", "3 — repo", "5 — webhook", "7 — template", "9 — install", "10 — webhook self-test", "12 — smoke"]) {
    assert.ok(failModes.includes(step), `failure-modes.md missing row for ${step}`);
  }
  const byok = readFileSync(resolve(skillDir, "references/byok-handoff.md"), "utf8");
  // Locked invariant: install-skill never holds an LLM api_key.
  assert.match(byok, /never holds an LLM (api[_ ]?key|API key)/i);
});
