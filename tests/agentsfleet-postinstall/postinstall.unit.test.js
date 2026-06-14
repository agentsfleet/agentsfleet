// Codified version of the manual smoke test for agentsfleet/scripts/postinstall.mjs.
// Every defensive path the script carries gets exercised against a temp HOME so
// regressions can't slip through silently. The script's contract is "never crash
// `npm install`" — these tests assert that and the idempotency invariant.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readFileSync, readdirSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..", "..");
const postinstall = resolve(repoRoot, "agentsfleet", "scripts", "postinstall.mjs");
const prepublish = resolve(repoRoot, "agentsfleet", "scripts", "prepublish.mjs");
const zombiectlDir = resolve(repoRoot, "agentsfleet");

function runNode(script, env = {}) {
  return spawnSync("node", [script], {
    cwd: zombiectlDir,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
}

function withTempHome(fn) {
  const home = mkdtempSync(join(tmpdir(), "skill-eval-home-"));
  try { return fn(home); } finally {
    try { chmodSync(home, 0o700); } catch {}
    rmSync(home, { recursive: true, force: true });
  }
}

function withBundledSamples(fn) {
  // Mirror repo-root samples/ into agentsfleet/samples/ via prepublish, run fn,
  // then clean up. Tests that need the bundled tree wrap with this helper.
  // Skills no longer bundle through prepublish — they live in
  // github.com/agentsfleet/skills and install via `npx skills add agentsfleet/skills`.
  const r = runNode(prepublish);
  if (r.status !== 0) throw new Error(`prepublish failed: ${r.stderr}`);
  try { return fn(); } finally {
    rmSync(resolve(zombiectlDir, "samples"), { recursive: true, force: true });
  }
}

test("postinstall in dev mode (no bundled samples) is a silent no-op", () => {
  // Ensure samples are NOT bundled.
  rmSync(resolve(zombiectlDir, "samples"), { recursive: true, force: true });
  withTempHome((home) => {
    const r = runNode(postinstall, { HOME: home });
    assert.equal(r.status, 0, "must exit 0 in dev mode");
    assert.equal(r.stdout.trim(), "", "must produce no stdout in dev mode");
    assert.equal(r.stderr.trim(), "", "must produce no stderr in dev mode");
    assert.equal(existsSync(resolve(home, ".config", "usezombie")), false);
  });
});

test("postinstall populates ~/.config/agentsfleet/samples/ on first run", () => {
  withBundledSamples(() => withTempHome((home) => {
    const r = runNode(postinstall, { HOME: home });
    assert.equal(r.status, 0);
    const dst = resolve(home, ".config", "usezombie", "samples");
    assert.ok(existsSync(resolve(dst, "platform-ops", "SKILL.md")));
    assert.ok(existsSync(resolve(dst, "platform-ops", "TRIGGER.md")));
    assert.ok(existsSync(resolve(home, ".config", "usezombie", ".samples-manifest")));
  }));
});

test("postinstall is idempotent on second run (manifest match → no-op)", () => {
  withBundledSamples(() => withTempHome((home) => {
    runNode(postinstall, { HOME: home });
    const r2 = runNode(postinstall, { HOME: home });
    assert.equal(r2.status, 0);
    assert.equal(r2.stdout.trim(), "", "second run must produce no stdout");
  }));
});

test("postinstall backs up a corrupted target before recopying", () => {
  withBundledSamples(() => withTempHome((home) => {
    // Pre-populate target with junk content to simulate corruption.
    const cfg = resolve(home, ".config", "usezombie");
    mkdirSync(resolve(cfg, "samples", "platform-ops"), { recursive: true });
    writeFileSync(resolve(cfg, "samples", "platform-ops", "SKILL.md"), "junk");
    // Manifest absent → trigger backup path.
    const r = runNode(postinstall, { HOME: home });
    assert.equal(r.status, 0);
    // Some backup-* dir exists alongside samples/.
    const backups = readdirSync(cfg).filter((n) => n.startsWith("samples.backup-"));
    assert.ok(backups.length >= 1, "expected at least one samples.backup-<ts> dir");
    // Fresh copy in place.
    assert.notEqual(readFileSync(resolve(cfg, "samples", "platform-ops", "SKILL.md"), "utf8"), "junk");
  }));
});

test("postinstall on a permission-denied HOME exits 0 (never crashes npm install)", () => {
  withBundledSamples(() => {
    const home = mkdtempSync(join(tmpdir(), "skill-eval-perm-"));
    try {
      chmodSync(home, 0o500); // read+execute, no write
      const r = runNode(postinstall, { HOME: home });
      assert.equal(r.status, 0, "must exit 0 even when HOME is unwritable");
      assert.match(r.stderr, /postinstall:.*permission denied|EACCES/i);
    } finally {
      chmodSync(home, 0o700);
      rmSync(home, { recursive: true, force: true });
    }
  });
});

test("prepublish bundles repo-root samples/ into the package dir", () => {
  withBundledSamples(() => {
    assert.ok(existsSync(resolve(zombiectlDir, "samples", "platform-ops", "SKILL.md")));
  });
});

test("prepublish scrubs a stray local skills/ from the package dir", () => {
  // A stale shell session could leave agentsfleet/skills/ behind from before
  // the skill bodies moved to their own repo. prepublish must actively remove
  // it so it can never sneak into the published tarball. Seed it, assert gone.
  const stray = resolve(zombiectlDir, "skills");
  mkdirSync(resolve(stray, "agentsfleet-install-platform-ops"), { recursive: true });
  writeFileSync(resolve(stray, "agentsfleet-install-platform-ops", "SKILL.md"), "stale\n");
  try {
    const r = runNode(prepublish);
    assert.equal(r.status, 0, `prepublish failed: ${r.stderr}`);
    assert.ok(!existsSync(stray), "stray skills/ should be scrubbed by prepublish");
  } finally {
    rmSync(stray, { recursive: true, force: true });
    rmSync(resolve(zombiectlDir, "samples"), { recursive: true, force: true });
  }
});
