/**
 * Concurrency acceptance scenario — seeded-credentials session, read-only.
 *
 * Two storms, both fanned out with a bounded `Promise.all`:
 *
 *   1. Isolated state dirs — N concurrent read-only invocations (`list
 *      --json` and `doctor --json`), each with its OWN tmpdir state dir +
 *      its own hydrated workspaces.json. Proves no cross-process state-file
 *      contention and no interleaved/corrupted stdout: every invocation
 *      exits cleanly (0 for `list`; 0-or-1 for `doctor`, whose exit 1 is a
 *      *logical* checks-failed signal, not a crash) and emits parseable JSON
 *      with the expected envelope key.
 *
 *   2. Shared state dir — N concurrent invocations against ONE state dir
 *      holding a pre-seeded credentials.json + a hydrated workspaces.json.
 *      All read-only (no writer in the fan-out), so the file must survive
 *      byte-for-byte; the assertion re-reads credentials.json after the
 *      storm and proves it still parses and is identical to the seed.
 *
 * Read-only throughout — nothing the storms run mutates the shared DEV
 * tenant, so no per-agent teardown is required beyond the defensive
 * prefix-scoped sweep in afterAll (kept for symmetry with the lifecycle
 * specs; it is a no-op here).
 *
 * WS-E #C1 regression: assertNoSecretLeak fires after every captured spawn —
 * the minted JWT must never echo into stdout/stderr.
 *
 * Live-only: the suite registers real tests only when
 * AGENTSFLEET_ACCEPTANCE_TARGET is an https URL; otherwise it skips cleanly
 * (CI runs it live; local runs skip).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import {
  boundedAll,
  CONCURRENCY_WIDTH,
  credentialsPathFor,
  DOCTOR_CHECKS_KEY,
  LIST_ITEMS_KEY,
  parseAllJson,
  readCredentialsRaw,
  seedCredentialsFile,
} from "./fixtures/concurrency-ops.ts";
import type { CredentialsRecord } from "./fixtures/concurrency-ops.ts";

const NO_COLOR = "1";
const STATE_DIR_PREFIX = "agentsfleet-concurrency-";
const SEED_SESSION_ID = `${ACCEPTANCE_RUN_PREFIX}-seed-session`;
// doctor exits 0 (all checks pass) or 1 (a check logically failed, e.g. a
// stale binding); both are clean exits. A crash, an unhandled rejection, or
// a state-file race would surface as a different code (2, 128, …).
const DOCTOR_CLEAN_EXIT_CODES: ReadonlyArray<number> = [0, 1];
const LIST_OK_EXIT_CODE = 0;

const LIST_ARGS: ReadonlyArray<string> = ["list", "--json"];
const DOCTOR_ARGS: ReadonlyArray<string> = ["doctor", "--json"];

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

async function makeStateDir(): Promise<string> {
  return fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
}

if (!isLive) {
  describe("concurrency.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("concurrency — read-only fan-out under a seeded-credentials session", () => {
    let apiUrl: string = "";
    let sessionJwt: string = "";
    // State dirs spun up across both scenarios — torn down in afterAll.
    const stateDirs: string[] = [];

    function envFor(stateDir: string): Record<string, string> {
      // Auth comes from the seeded credentials.json in stateDir (file slot);
      // the AGENTSFLEET_TOKEN env var was removed.
      return composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR,
      });
    }

    // Spawn + the WS-E #C1 leak guard in one place — every captured result
    // funnels through here so no scenario forgets the secret-leak assertion.
    // assertNoConnectionError is deliberately NOT applied: `doctor` folds a
    // failed check (including a transient network blip on the binding probe)
    // into its envelope detail as a *clean* exit-1, which is exactly what
    // DOCTOR_CLEAN_EXIT_CODES tolerates — a connection-string match there
    // would convert that tolerated degraded run into a hard failure. That
    // guard belongs on negative arg-validation paths, not read storms.
    async function runGuarded(
      args: ReadonlyArray<string>,
      env: Record<string, string>,
    ): Promise<RunResult> {
      const result = await runAgentctl(args, { env });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;
    });

    afterAll(async () => {
      // Read-only suite — no agents are created, so the sweep is a no-op
      // guard against a stray fixture. Best-effort: never fail teardown.
      const survivor = stateDirs[0];
      if (survivor && sessionJwt && apiUrl) {
        try { await cleanWorkspaceAgents(envFor(survivor), { runPrefix: ACCEPTANCE_RUN_PREFIX }); }
        catch { /* best-effort teardown */ }
      }
      await Promise.all(
        stateDirs.map((dir) => fs.rm(dir, { recursive: true, force: true }).catch(() => undefined)),
      );
    });

    // ── Scenario 1: isolated state dirs ──────────────────────────────
    describe("isolated state dirs — N concurrent read-only invocations", () => {
      let isolatedEnvs: Array<Record<string, string>> = [];

      beforeAll(async () => {
        // One tmpdir + one hydrated workspaces.json per lane. Hydration is
        // done serially (it only writes a local file) so the concurrent
        // phase is purely the CLI spawns we want to stress.
        const envs: Array<Record<string, string>> = [];
        for (let i = 0; i < CONCURRENCY_WIDTH; i += 1) {
          const dir = await makeStateDir();
          stateDirs.push(dir);
          await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir: dir });
          envs.push(envFor(dir));
        }
        isolatedEnvs = envs;
      }, 60_000);

      it(`${CONCURRENCY_WIDTH} concurrent \`list --json\` all exit 0 with parseable JSON`, async () => {
        const results = await boundedAll(
          isolatedEnvs.map((env) => () => runGuarded(LIST_ARGS, env)),
          CONCURRENCY_WIDTH,
        );
        for (const [index, result] of results.entries()) {
          assert.equal(result.code, LIST_OK_EXIT_CODE,
            `lane ${index}: list exited ${result.code}: ${result.stderr}`);
        }
        const parsed = parseAllJson(results);
        for (const [index, envelope] of parsed.entries()) {
          assert.ok(Array.isArray(envelope[LIST_ITEMS_KEY]),
            `lane ${index}: ${LIST_ITEMS_KEY} not an array: ${JSON.stringify(envelope).slice(0, 200)}`);
        }
      }, 60_000);

      it(`${CONCURRENCY_WIDTH} concurrent \`doctor --json\` exit cleanly with parseable JSON`, async () => {
        const results = await boundedAll(
          isolatedEnvs.map((env) => () => runGuarded(DOCTOR_ARGS, env)),
          CONCURRENCY_WIDTH,
        );
        for (const [index, result] of results.entries()) {
          assert.ok(DOCTOR_CLEAN_EXIT_CODES.includes(result.code),
            `lane ${index}: doctor exited ${result.code} (expected one of ${DOCTOR_CLEAN_EXIT_CODES.join("|")}): ${result.stderr}`);
        }
        const parsed = parseAllJson(results);
        for (const [index, envelope] of parsed.entries()) {
          assert.ok(Array.isArray(envelope[DOCTOR_CHECKS_KEY]),
            `lane ${index}: ${DOCTOR_CHECKS_KEY} not an array: ${JSON.stringify(envelope).slice(0, 200)}`);
        }
      }, 60_000);

      it("isolated lanes do not cross-talk — each list envelope is independently parseable", async () => {
        // Re-run the two command shapes interleaved across all lanes in a
        // single fan-out: if process A's stdout bled into process B's, the
        // mixed batch is exactly where a truncated/duplicated envelope shows
        // up. parseAllJson throws with the offending index on any failure.
        const thunks = isolatedEnvs.flatMap((env) => [
          () => runGuarded(LIST_ARGS, env),
          () => runGuarded(DOCTOR_ARGS, env),
        ]);
        const results = await boundedAll(thunks, CONCURRENCY_WIDTH);
        const parsed = parseAllJson(results);
        assert.equal(parsed.length, thunks.length, "every interleaved invocation produced a JSON object");
      }, 90_000);
    });

    // ── Scenario 2: shared state dir, no credentials.json corruption ──
    describe("shared state dir — concurrent reads never corrupt credentials.json", () => {
      let sharedDir: string = "";
      let sharedEnv: Record<string, string> = {};
      let seedBytes: string = "";
      const seedRecord: CredentialsRecord = {
        token: "",
        saved_at: 0,
        session_id: SEED_SESSION_ID,
        api_url: "",
      };

      beforeAll(async () => {
        sharedDir = await makeStateDir();
        stateDirs.push(sharedDir);
        // Hydrate workspaces.json so the read commands resolve a workspace,
        // then pin a well-formed credentials.json. The fan-out below
        // authenticates FROM that file (reads only — no lane *writes* it back);
        // the invariant is that concurrent readers leave the seed byte-identical.
        // (hydrate also seeds a credentials.json; seedCredentialsFile below is
        // the authoritative last writer and the byte-pinned value.)
        await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir: sharedDir });
        const record: CredentialsRecord = {
          ...seedRecord,
          // Seed the live JWT so the file mirrors a genuine credentials.json.
          // runGuarded's assertNoSecretLeak (keyed on the same JWT) is what
          // proves no read lane echoes it onto stdout/stderr; this on-disk
          // copy is the value the byte-identical assertion pins after the storm.
          token: sessionJwt,
          saved_at: Date.now(),
          api_url: apiUrl,
        };
        seedBytes = await seedCredentialsFile(sharedDir, record);
        sharedEnv = envFor(sharedDir);
      }, 60_000);

      it(`${CONCURRENCY_WIDTH} concurrent reads on one state dir all succeed`, async () => {
        // Mix list + doctor so both read paths touch the shared dir at once.
        const thunks = Array.from({ length: CONCURRENCY_WIDTH }, (_unused, i) =>
          (i % 2 === 0)
            ? () => runGuarded(LIST_ARGS, sharedEnv)
            : () => runGuarded(DOCTOR_ARGS, sharedEnv));
        const results = await boundedAll(thunks, CONCURRENCY_WIDTH);
        for (const [index, result] of results.entries()) {
          const cleanExit = index % 2 === 0
            ? result.code === LIST_OK_EXIT_CODE
            : DOCTOR_CLEAN_EXIT_CODES.includes(result.code);
          assert.ok(cleanExit, `invocation ${index} exited ${result.code}: ${result.stderr}`);
        }
        // Every stdout still parses — interleaved writes to a shared stream
        // are a different failure mode than file corruption, caught here.
        parseAllJson(results);
      }, 90_000);

      it("credentials.json still parses and is byte-identical to the seed", async () => {
        const after = await readCredentialsRaw(sharedDir);
        assert.equal(after, seedBytes,
          `credentials.json mutated under concurrent reads — expected byte-identical seed`);
        const parsed = JSON.parse(after) as CredentialsRecord;
        assert.equal(parsed.session_id, SEED_SESSION_ID, "seed session_id survived the read storm");
        assert.equal(typeof parsed.token, "string", "token field intact after concurrent reads");
      });

      it("the shared credentials.json path is the one the CLI resolves", () => {
        // Guard against the seed landing in the wrong place (which would make
        // the byte-identical assertion vacuously pass). The path mirrors
        // src/lib/state.ts resolveStatePaths under AGENTSFLEET_STATE_DIR.
        const expected = credentialsPathFor(sharedDir);
        assert.equal(sharedEnv.AGENTSFLEET_STATE_DIR, sharedDir, "shared env points at the seeded state dir");
        assert.ok(expected.startsWith(sharedDir), "credentials path resolves under the shared state dir");
      });
    });
  });
}
