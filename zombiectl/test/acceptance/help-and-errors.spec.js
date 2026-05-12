/**
 * Unauth-surface acceptance scenario.
 *
 * Proves the CLI's parse + help + auth-guard layer behaves correctly
 * against the same binary that ships to prod (worktree-DEV /
 * npm-global-PROD). No `ZOMBIE_TOKEN`, no `credentials.json`, no live
 * API calls (the auth guard fires before any network I/O — the suite
 * sets `ZOMBIE_API_URL` to an unroutable address so a leaked fetch
 * surfaces as ECONNREFUSED instead of the expected stem).
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  COMMAND_GROUPS,
  AUTH_REQUIRED_REPRESENTATIVE,
} from "./fixtures/command-matrix.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.js";
import { runZombiectl, composeEnv } from "./fixtures/cli.js";
import { makeStubbedStateDir } from "./fixtures/state-dir.js";
import {
  expectInvalidSubcommand,
  assertNoConnectionError,
} from "./fixtures/negatives.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..", "..");

const ANSI_RE = /\x1b\[[0-9;]*m/g;

function stripAnsi(text) {
  return text.replace(ANSI_RE, "").replace(/\s+$/gm, "");
}

let pkgVersion;
let validateModule;

before(async () => {
  const pkgRaw = await fs.readFile(path.join(ZOMBIECTL_ROOT, "package.json"), "utf8");
  pkgVersion = JSON.parse(pkgRaw).version;
  validateModule = await import(path.join(ZOMBIECTL_ROOT, "src/program/validate.js"));
});

function emptyEnv(extra) {
  return composeEnv({ ZOMBIE_API_URL: UNROUTABLE_API_URL, NO_COLOR: "1", ...(extra ?? {}) });
}

describe("help triplet", () => {
  const invocations = [[], ["help"], ["-h"], ["--help"]];

  it("all four forms exit 0 and contain the help banner", async () => {
    const results = await Promise.all(
      invocations.map((args) => runZombiectl(args, { env: emptyEnv() })),
    );
    for (let i = 0; i < invocations.length; i += 1) {
      assert.equal(results[i].code, 0, `${JSON.stringify(invocations[i])} exited ${results[i].code}; stderr=${results[i].stderr}`);
      assert.match(results[i].stdout, /zombiectl/i, `${JSON.stringify(invocations[i])} missing banner`);
    }
  });

  it("all four forms emit byte-identical stripped stdout", async () => {
    const results = await Promise.all(
      invocations.map((args) => runZombiectl(args, { env: emptyEnv() })),
    );
    const stripped = results.map((r) => stripAnsi(r.stdout));
    for (let i = 1; i < stripped.length; i += 1) {
      assert.equal(
        stripped[i],
        stripped[0],
        `help-form ${JSON.stringify(invocations[i])} drifted from bare zombiectl`,
      );
    }
  });
});

describe("--version", () => {
  it("--version exits 0 with stdout containing the package.json version", async () => {
    const result = await runZombiectl(["--version"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(
      result.stdout.includes(pkgVersion),
      `expected ${pkgVersion} in stdout: ${result.stdout}`,
    );
  });

  it("-v is equivalent to --version", async () => {
    const result = await runZombiectl(["-v"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion));
  });

  it("--version --json emits parseable JSON with the version field", async () => {
    const result = await runZombiectl(["--version", "--json"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    const parsed = JSON.parse(result.stdout.trim());
    assert.equal(parsed.version, pkgVersion);
  });
});

describe("unknown commands", () => {
  let stubState;

  before(async () => {
    stubState = await makeStubbedStateDir();
  });

  after(async () => {
    if (stubState) await stubState.cleanup();
  });

  it("unknown top-level command exits non-zero with suggest stem", async () => {
    const result = await runZombiectl(["pogo"], { env: emptyEnv() });
    assert.notEqual(result.code, 0);
    assert.match(result.stderr.toLowerCase(), /unknown|did you mean|usage/);
  });

  // Per-group `<group> pogo` reaches the group's dispatcher only after
  // auth + workspace-context resolution. The stubbed state-dir threads a
  // syntactically-valid-but-never-used token + workspace through that
  // gate. `ZOMBIE_API_URL=http://127.0.0.1:1` ensures any accidental fetch
  // surfaces as ECONNREFUSED — the dispatcher's "unknown action" branch
  // fires before any network attempt.
  for (const group of COMMAND_GROUPS) {
    it(`unknown subcommand on "${group}" exits non-zero (no network)`, async () => {
      const env = emptyEnv({ ZOMBIE_STATE_DIR: stubState.dir });
      const result = await expectInvalidSubcommand(group, env);
      assertNoConnectionError(result, [group, "pogo"]);
    });
  }
});

// NOTE: Missing-required-arg tests live in `lifecycle-with-token.spec.js`
// rather than here. The current CLI's command handlers check workspace-
// context (auth + workspaces.json) BEFORE arg-validation, so a missing-arg
// invocation without state lands at "no workspace selected" instead of
// "missing argument". The lifecycle suite has a workspace context naturally,
// so the missing-arg sweep runs there. The dispatcher ordering itself is a
// Discovery item for a follow-on CLI hygiene PR.

describe("auth guard short-circuits before any network call", () => {
  for (const args of AUTH_REQUIRED_REPRESENTATIVE) {
    it(`"${args.join(" ")}" exits 1 with "not authenticated", no ECONNREFUSED`, async () => {
      const result = await runZombiectl(args, { env: emptyEnv() });
      assert.equal(result.code, 1, `expected exit 1; got ${result.code}; stderr=${result.stderr}`);
      const merged = `${result.stderr}\n${result.stdout}`.toLowerCase();
      assert.match(merged, /not authenticated|authentication required|please.*log/);
      assertNoConnectionError(result, args);
    });
  }
});

describe("validate.js error stem", () => {
  // "abc def" fails the current validator because spaces aren't allowed
  // in SAFE_ID_RE. "not-a-uuid" actually passes today (matches SAFE_ID_RE);
  // that mismatch with the backend's strict uuidv7 is the motivation for
  // the optional library swap covered in the lifecycle suite.
  it("validateRequiredId is reachable and emits the invalid-format stem", () => {
    const result = validateModule.validateRequiredId("abc def", "zombie_id");
    assert.equal(result.ok, false);
    assert.match(
      result.message,
      /invalid zombie_id: expected/i,
      `unexpected stem: ${result.message}`,
    );
  });

  it("validateRequiredId('') reports the empty-required stem", () => {
    const result = validateModule.validateRequiredId("", "workspace_id");
    assert.equal(result.ok, false);
    assert.match(result.message, /workspace_id is required/i);
  });
});
