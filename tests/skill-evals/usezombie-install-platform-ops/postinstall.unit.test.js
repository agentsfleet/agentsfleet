// Codified version of the manual smoke test for zombiectl/scripts/postinstall.mjs.
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
const repoRoot = resolve(__dirname, "..", "..", "..");
const postinstall = resolve(repoRoot, "zombiectl", "scripts", "postinstall.mjs");
const prepublish = resolve(repoRoot, "zombiectl", "scripts", "prepublish.mjs");
const zombiectlDir = resolve(repoRoot, "zombiectl");

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
  // Mirror repo-root samples/ into zombiectl/samples/ via prepublish, run fn,
  // then clean up. Tests that need the bundled tree wrap with this helper.
  const r = runNode(prepublish);
  if (r.status !== 0) throw new Error(`prepublish failed: ${r.stderr}`);
  try { return fn(); } finally {
    rmSync(resolve(zombiectlDir, "samples"), { recursive: true, force: true });
    rmSync(resolve(zombiectlDir, "skills"), { recursive: true, force: true });
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

test("postinstall populates ~/.config/usezombie/samples/ on first run", () => {
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

test("prepublish bundles repo-root samples/ + skills/ into the package dir", () => {
  withBundledSamples(() => {
    assert.ok(existsSync(resolve(zombiectlDir, "samples", "platform-ops", "SKILL.md")));
    assert.ok(existsSync(resolve(zombiectlDir, "skills", "usezombie-install-platform-ops", "SKILL.md")));
  });
});
