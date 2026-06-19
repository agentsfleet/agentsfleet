/**
 * Unauth-surface acceptance scenario.
 *
 * Proves the CLI's parse + help + auth-guard layer behaves correctly
 * against the same binary that ships to prod (worktree-DEV /
 * npm-global-PROD). No `AGENTSFLEET_TOKEN`, no `credentials.json`, no live
 * API calls (the auth guard fires before any network I/O — the suite
 * sets `AGENTSFLEET_API_URL` to an unroutable address so a leaked fetch
 * surfaces as ECONNREFUSED instead of the expected stem).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  COMMAND_GROUPS,
  AUTH_REQUIRED_REPRESENTATIVE,
} from "./fixtures/command-matrix.ts";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { runAgentctl, composeEnv } from "./fixtures/cli.js";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";
import {
  expectInvalidSubcommand,
  assertNoConnectionError,
} from "./fixtures/negatives.ts";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const CLI_ROOT = path.resolve(HERE, "..", "..");

const ANSI_RE = /\x1b\[[0-9;]*m/g;

function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, "").replace(/\s+$/gm, "");
}

interface ValidateResult {
  readonly ok: boolean;
  readonly message: string;
}

interface ValidateModule {
  validateRequiredId(value: string, label: string): ValidateResult;
}

let pkgVersion: string;
let validateModule: ValidateModule;

beforeAll(async () => {
  const pkgRaw = await fs.readFile(path.join(CLI_ROOT, "package.json"), "utf8");
  pkgVersion = (JSON.parse(pkgRaw) as { version: string }).version;
  validateModule = await import(path.join(CLI_ROOT, "src/program/validators.ts")) as ValidateModule;
});

function emptyEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({ AGENTSFLEET_API_URL: UNROUTABLE_API_URL, NO_COLOR: "1", ...(extra ?? {}) });
}

describe("help triplet", () => {
  const invocations: ReadonlyArray<ReadonlyArray<string>> = [[], ["help"], ["-h"], ["--help"]];

  it("all four forms exit 0 and contain the help banner", async () => {
    const results = await Promise.all(
      invocations.map((args) => runAgentctl(args, { env: emptyEnv() })),
    );
    invocations.forEach((args, i) => {
      const r = results[i];
      if (!r) throw new Error(`missing result index ${i}`);
      assert.equal(r.code, 0, `${JSON.stringify(args)} exited ${r.code}; stderr=${r.stderr}`);
      assert.match(r.stdout, /agentsfleet/i, `${JSON.stringify(args)} missing banner`);
    });
  });

  it("all four forms emit byte-identical stripped stdout", async () => {
    const results = await Promise.all(
      invocations.map((args) => runAgentctl(args, { env: emptyEnv() })),
    );
    const stripped = results.map((r) => stripAnsi(r.stdout));
    const first = stripped[0];
    if (first === undefined) throw new Error("no help invocations recorded");
    for (let i = 1; i < stripped.length; i += 1) {
      assert.equal(
        stripped[i],
        first,
        `help-form ${JSON.stringify(invocations[i])} drifted from bare agentsfleet`,
      );
    }
  });
});

describe("--version", () => {
  it("--version exits 0 with stdout containing the package.json version", async () => {
    const result = await runAgentctl(["--version"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(
      result.stdout.includes(pkgVersion),
      `expected ${pkgVersion} in stdout: ${result.stdout}`,
    );
  });

  it("-v is equivalent to --version", async () => {
    const result = await runAgentctl(["-v"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    assert.ok(result.stdout.includes(pkgVersion));
  });

  it("--version --json emits parseable JSON with the version field", async () => {
    const result = await runAgentctl(["--version", "--json"], { env: emptyEnv() });
    assert.equal(result.code, 0);
    const parsed = JSON.parse(result.stdout.trim()) as { version: string };
    assert.equal(parsed.version, pkgVersion);
  });
});

describe("unknown commands", () => {
  let stubState: StubbedStateDir | null = null;

  beforeAll(async () => {
    stubState = await makeStubbedStateDir();
  });

  afterAll(async () => {
    if (stubState) await stubState.cleanup();
  });

  it("unknown top-level command exits non-zero with suggest stem", async () => {
    const result = await runAgentctl(["pogo"], { env: emptyEnv() });
    assert.notEqual(result.code, 0);
    assert.match(result.stderr.toLowerCase(), /unknown|did you mean|usage/);
  });

  // Per-group `<group> pogo` reaches the group's dispatcher only after
  // auth + workspace-context resolution. The stubbed state-dir threads a
  // syntactically-valid-but-never-used token + workspace through that
  // gate. `AGENTSFLEET_API_URL=http://127.0.0.1:1` ensures any accidental fetch
  // surfaces as ECONNREFUSED — the dispatcher's "unknown action" branch
  // fires before any network attempt.
  for (const group of COMMAND_GROUPS) {
    it(`unknown subcommand on "${group}" exits non-zero (no network)`, async () => {
      if (!stubState) throw new Error("stubState not initialised");
      const env = emptyEnv({ AGENTSFLEET_STATE_DIR: stubState.dir });
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
      const result = await runAgentctl(args, { env: emptyEnv() });
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
    const result = validateModule.validateRequiredId("abc def", "agent_id");
    assert.equal(result.ok, false);
    assert.match(
      result.message,
      /invalid agent_id: expected/i,
      `unexpected stem: ${result.message}`,
    );
  });

  it("validateRequiredId('') reports the empty-required stem", () => {
    const result = validateModule.validateRequiredId("", "workspace_id");
    assert.equal(result.ok, false);
    assert.match(result.message, /workspace_id is required/i);
  });
});

// Integration coverage through the REAL shipped binary (worktree-DEV /
// npm-global-PROD), not the in-process renderer. cli-tree-help.unit.test.ts
// asserts helpTail() in isolation; these prove the same guarantees survive
// the build, commander wiring, and process boundary — a bundling or
// help-registration regression the unit test can't see.
describe("help DX surfaces (real binary)", () => {
  function helpEnv(): Record<string, string> {
    // Disable telemetry so a PostHog flush timeout can't add stderr noise
    // or stretch the spawn; help must not depend on network either way.
    return emptyEnv({ AGENTSFLEET_TELEMETRY_DISABLED: "1" });
  }

  it("--help renders the task-grouped command guide", async () => {
    const result = await runAgentctl(["--help"], { env: helpEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const out = stripAnsi(result.stdout);
    assert.match(out, /Commands by task:/);
    for (const title of ["Setup", "Workspaces", "Agents", "Tenant & billing", "Memory"]) {
      assert.ok(out.includes(title), `guide missing "${title}" group`);
    }
    // The lone agent-group verb surfaced in the guide (the split made explicit).
    assert.ok(out.includes("agent update"), "guide missing `agent update`");
  });

  it("--help aligns every environment-variable description to one column", async () => {
    const result = await runAgentctl(["--help"], { env: helpEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const lines = stripAnsi(result.stdout).split("\n");
    const start = lines.indexOf("Environment variables:");
    assert.ok(start >= 0, "no Environment variables section in --help");
    const columns: number[] = [];
    for (const line of lines.slice(start + 1)) {
      const match = /^ {2}(\S+)( +)(\S.*)$/.exec(line);
      const name = match?.[1];
      const gap = match?.[2];
      if (name !== undefined && gap !== undefined) columns.push(2 + name.length + gap.length);
    }
    assert.ok(columns.length >= 11, `expected ≥11 env rows, got ${columns.length}`);
    assert.equal(new Set(columns).size, 1, `env descriptions misaligned: columns=${columns.join(",")}`);
  });

  it("--help stays within 80 columns through the real binary", async () => {
    const result = await runAgentctl(["--help"], { env: helpEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const overflow = stripAnsi(result.stdout).split("\n").filter((l) => l.length > 80);
    assert.deepEqual(overflow, [], `lines over 80 cols: ${overflow.join(" | ")}`);
  });

  it("agent --help clarifies the lifecycle verbs are top-level", async () => {
    const result = await runAgentctl(["agent", "--help"], { env: helpEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const out = stripAnsi(result.stdout);
    assert.match(out, /top-level commands/);
    assert.ok(out.includes("agentsfleet --help"), "agent --help should point at the full guide");
  });
});
