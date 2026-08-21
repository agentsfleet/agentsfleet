/**
 * steer-live — one-shot `steer` against a freshly-installed live fleet.
 *
 * Scenario (seeded-credentials session, mirrors lifecycle-with-token.spec.ts):
 *   - mint a Clerk session JWT via the admin path
 *   - hydrate workspaces.json directly from the API (the CLI only
 *     hydrates inside the login flow)
 *   - install the platform-ops bundle (prefix-scoped name)
 *   - drive `steer <fleet_id> <message> --json` non-interactively — the
 *     spawned child's stdin is a pipe AND a positional message is
 *     supplied, so `shouldEnterSteerRepl` (message===undefined && tty)
 *     stays false and the command runs a single turn (no REPL drive)
 *   - require the steer envelope to contain a processed terminal result. The CLI streams content
 *     frames to STDOUT as plain `[claw] …` / `[tool] …` lines via
 *     `output.info` even under `--json`, then writes the JSON envelope
 *     LAST via `output.printJson`. So the envelope is the trailing
 *     balanced `{…}` object in stdout — parsing the *whole* stdout as
 *     JSON would throw on any turn that streamed prose. Acceptance:
 *       - exit 0 → `{event_id, kind: "complete", status: "processed"}`
 *       - every timeout, cancellation, and non-processed result fails.
 *
 * Negative paths (no network residue beyond the already-installed fleet):
 *   - whitespace-only message rejected client-side ("message is required")
 *   - missing `<fleet_id>` rejected by commander before any network call
 *
 * Teardown: prefix-scoped `cleanWorkspaceFleets` — only this run's fleets
 * are killed; shared-tenant residue from other runs is left untouched and
 * global emptiness is never asserted.
 *
 * The minted JWT must not appear in any spawn's stdout/stderr
 * (`assertNoSecretLeak` after every `runFleetctl`).
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET`
 * is an https URL; otherwise every test is skipped (local runs skip; CI
 * runs them live).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installSteerProbeFleet } from "./fixtures/seed.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// Wire/output literals (RULE UFS — each crosses a boundary or repeats).
const STEER_COMMAND = "steer" as const;
const JSON_FLAG = "--json" as const;
const ENVELOPE_EVENT_ID_KEY = "event_id" as const;
const ENVELOPE_KIND_KEY = "kind" as const;
const ENVELOPE_STATUS_KEY = "status" as const;
const KIND_COMPLETE = "complete" as const;
const STATUS_PROCESSED = "processed" as const;
const STATE_DIR_PREFIX = "agentsfleet-steer-" as const;
const ONE_SHOT_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const WHITESPACE_MESSAGE = "   " as const;
// An id no provider serves. Deliberately shaped like a real Fireworks path so
// the fleet installs and reaches its first model call — a malformed string
// could be rejected earlier by validation and would prove nothing about the
// dial. `k2.6` is the exact mistype this negative exists for: Fireworks writes
// the decimal as `p` (`kimi-k2p6`), and the dotted spelling sat in this suite's
// own fixtures for months.
const NONEXISTENT_MODEL = "accounts/fireworks/models/kimi-k2.6" as const;
const STATUS_FLEET_ERROR = "fleet_error" as const;
const NO_COLOR = "1" as const;
const OPEN_BRACE = "{" as const;
const CLOSE_BRACE = "}" as const;
const QUOTE = '"' as const;
const BACKSLASH = "\\" as const;

// The SSE round-trip falls back to a ~60s poll window before declaring a
// timeout, then renders. Budget well above that so a slow-but-valid turn
// reads as `complete`; `runFleetctl` *throws* TimeoutError if the child
// outlives this, so it must exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;

interface SteerEnvelope {
  readonly [ENVELOPE_EVENT_ID_KEY]?: unknown;
  readonly [ENVELOPE_KIND_KEY]?: unknown;
  readonly [ENVELOPE_STATUS_KEY]?: unknown;
}

// Extract the trailing balanced `{…}` object from stdout. The CLI
// interleaves `[claw] …` / `[tool] …` content frames (which may contain
// braces) before the final pretty-printed JSON envelope, so a naive
// `JSON.parse(stdout)` would throw. Scan from the last `}` back to its
// depth-0 `{`, ignoring braces inside string literals.
function trailingJsonObject(stdout: string): string {
  const end = stdout.lastIndexOf(CLOSE_BRACE);
  assert.ok(end >= 0, `steer --json produced no JSON object: ${stdout}`);
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = end; i >= 0; i--) {
    const ch = stdout[i];
    if (escaped) { escaped = false; continue; }
    if (inString) {
      if (ch === BACKSLASH) { escaped = true; continue; }
      if (ch === QUOTE) inString = false;
      continue;
    }
    if (ch === QUOTE) { inString = true; continue; }
    if (ch === CLOSE_BRACE) depth++;
    else if (ch === OPEN_BRACE) {
      depth--;
      if (depth === 0) return stdout.slice(i, end + 1);
    }
  }
  throw new assert.AssertionError({ message: `unbalanced JSON in steer stdout: ${stdout}` });
}

function parseSteerEnvelope(stdout: string): SteerEnvelope {
  const raw = trailingJsonObject(stdout);
  const parsed = JSON.parse(raw) as SteerEnvelope;
  assert.equal(typeof parsed, "object", `steer envelope is not an object: ${raw}`);
  assert.ok(parsed !== null, "steer envelope is null");
  return parsed;
}

function assertSteerProcessed(result: RunResult, diagnostics = ""): void {
  assert.equal(result.code, 0,
    `live steer must exit 0 with a processed result; stdout=${result.stdout} stderr=${result.stderr}${diagnostics}`);
  const envelope = parseSteerEnvelope(result.stdout);
  const eventId = envelope[ENVELOPE_EVENT_ID_KEY];
  assert.equal(typeof eventId, "string", `steer envelope missing ${ENVELOPE_EVENT_ID_KEY}: ${result.stdout}`);
  assert.ok((eventId as string).length > 0, `steer ${ENVELOPE_EVENT_ID_KEY} is empty`);
  assert.equal(envelope[ENVELOPE_KIND_KEY], KIND_COMPLETE,
    `live steer must carry kind=${KIND_COMPLETE}; got ${JSON.stringify(envelope)}`);
  assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
    `live steer must carry status=${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);
}

if (!isLive) {
  describe("steer-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("steer-live — one-shot steer against a live fleet", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let fleetId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: STEER_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: NO_COLOR,
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;

      const installed = await installSteerProbeFleet({ env, seedFixtureSecrets: false });
      const id = installed.id ?? installed.fleet_id;
      if (!id) throw new Error(`install missing id: ${JSON.stringify(installed)}`);
      fleetId = id;
    }, STEER_TIMEOUT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("steer <id> <message> --json returns a processed terminal result", async () => {
      assert.ok(fleetId, "fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      const diagnostics = result.code === 0
        ? ""
        : ` events=${JSON.stringify(await runWithEnv(["events", fleetId, JSON_FLAG]))}`;
      assertSteerProcessed(result, diagnostics);
    }, STEER_TIMEOUT_MS);

    it("steer <id> with a whitespace-only message is rejected client-side", async () => {
      assert.ok(fleetId, "fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, WHITESPACE_MESSAGE, JSON_FLAG]);
      assert.notEqual(result.code, 0, `expected non-zero; stdout=${result.stdout} stderr=${result.stderr}`);
      assert.match(`${result.stderr}\n${result.stdout}`, /message is required/i,
        `expected "message is required" stem; got stdout=${result.stdout} stderr=${result.stderr}`);
    });

    it("a fleet pinned to a model no provider serves fails terminally, not silently", async () => {
      // The failure this suite could not see until the runner's spawn was
      // fixed: every live steer used to die BEFORE the provider was dialed, so
      // a fixture pinned to a non-existent model looked identical to a broken
      // sandbox. Now the dial happens, and this is what a mistyped model does.
      //
      // What it must NOT do is look like success: `steer` exits non-zero and
      // the event reaches a TERMINAL error status rather than hanging or
      // reporting `processed` with an empty reply.
      const bad = await installSteerProbeFleet({
        env,
        seedFixtureSecrets: false,
        model: NONEXISTENT_MODEL,
      });
      const badId = bad.id ?? bad.fleet_id;
      assert.ok(badId, `install missing id: ${JSON.stringify(bad)}`);

      const result = await runWithEnv([STEER_COMMAND, badId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assert.notEqual(result.code, 0,
        `a fleet on a non-existent model must not exit 0; stdout=${result.stdout} stderr=${result.stderr}`);
      const envelope = parseSteerEnvelope(result.stdout);
      assert.equal(envelope[ENVELOPE_KIND_KEY], KIND_COMPLETE,
        `expected a terminal envelope, not a hang; got ${JSON.stringify(envelope)}`);
      assert.notEqual(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
        `a model that does not exist must never report ${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);
      assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_FLEET_ERROR,
        `expected ${STATUS_FLEET_ERROR}; got ${JSON.stringify(envelope)}`);

      // The cause line must be DIAGNOSABLE, which is the half that cost days:
      // `ApiError` alone is indistinguishable from a rejected credential,
      // because nullclaw's `error_classify` collapses both into one bucket.
      // The runner now carries the provider's own words onto the event, so the
      // 404 and its "model not found" wording have to survive to here.
      //
      // Runner-version-dependent by nature: this asserts the behaviour of the
      // runner deployed to the target, so it goes green once the worker deploy
      // in this pipeline lands. A bare `ApiError` here means the target is
      // still running a pre-fix runner.
      const events = await runWithEnv(["events", badId, JSON_FLAG]);
      assert.equal(events.code, 0, `events read failed: ${events.stderr}`);
      assert.match(events.stdout, /not found|does not exist|unknown model/i,
        `the failure must say WHY, not just "ApiError": ${events.stdout}`);
    }, STEER_TIMEOUT_MS);

    it("steer with no <fleet_id> exits non-zero with a usage stem", async () => {
      const result = await runWithEnv([STEER_COMMAND, JSON_FLAG]);
      assert.notEqual(result.code, 0, `expected non-zero; stdout=${result.stdout} stderr=${result.stderr}`);
      assert.match(`${result.stderr}\n${result.stdout}`.toLowerCase(), /missing|required|usage|expected/,
        `expected a missing-arg stem; got stdout=${result.stdout} stderr=${result.stderr}`);
    });
  });
}
