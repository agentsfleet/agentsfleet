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
 *   - assert the steer envelope is accepted. The CLI streams content
 *     frames to STDOUT as plain `[claw] …` / `[tool] …` lines via
 *     `output.info` even under `--json`, then writes the JSON envelope
 *     LAST via `output.printJson`. So the envelope is the trailing
 *     balanced `{…}` object in stdout — parsing the *whole* stdout as
 *     JSON would throw on any turn that streamed prose. Acceptance:
 *       - exit 0 → `{event_id, kind: "complete", status: "processed"}`
 *       - exit !=0 → a graceful non-processed terminal / timeout on a
 *         shared DEV tenant (credits, gate, runner latency) that still
 *         names the event in the envelope and carries the matching
 *         `renderError` stem on stderr. Tolerated as long as the minted
 *         JWT never leaks.
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
import { installPlatformOpsFleet } from "./fixtures/seed.ts";
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
const KIND_TIMEOUT = "timeout" as const;
const STATUS_PROCESSED = "processed" as const;
const STATE_DIR_PREFIX = "agentsfleet-steer-" as const;
const ONE_SHOT_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const WHITESPACE_MESSAGE = "   " as const;
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

// Non-zero exits emit a typed CliError whose `message` (detail) reaches
// stderr via `renderError` → `output.error`. The two single-turn stems
// (`renderOutcome`): a non-processed terminal completion and a timeout.
// The `still in flight` line is a JSON-mode-suppressed branch and never
// fires here, so it is intentionally absent from this matcher.
const TOLERATED_TERMINAL_STEM = /event\s+\S+\s+(terminated with status|did not complete)/i;

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

// Accept either path the live server can drive on a shared DEV tenant:
//   - exit 0 → `kind: complete`, `status: processed`
//   - exit !=0 → a graceful non-processed terminal / timeout that still
//     names the event and carries the matching `renderError` stem
function assertSteerAccepted(result: RunResult): void {
  const envelope = parseSteerEnvelope(result.stdout);
  const eventId = envelope[ENVELOPE_EVENT_ID_KEY];
  assert.equal(typeof eventId, "string", `steer envelope missing ${ENVELOPE_EVENT_ID_KEY}: ${result.stdout}`);
  assert.ok((eventId as string).length > 0, `steer ${ENVELOPE_EVENT_ID_KEY} is empty`);

  if (result.code === 0) {
    assert.equal(envelope[ENVELOPE_KIND_KEY], KIND_COMPLETE,
      `exit 0 must carry kind=${KIND_COMPLETE}; got ${JSON.stringify(envelope)}`);
    assert.equal(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
      `exit 0 must carry status=${STATUS_PROCESSED}; got ${JSON.stringify(envelope)}`);
    return;
  }

  // Non-zero: a `complete`/`processed` envelope here is a contradiction
  // (success shape, failure code) and must NOT pass. A `complete` with a
  // non-processed terminal status, or a `timeout`, is the graceful path.
  const kind = envelope[ENVELOPE_KIND_KEY];
  if (kind === KIND_COMPLETE) {
    assert.notEqual(envelope[ENVELOPE_STATUS_KEY], STATUS_PROCESSED,
      `processed completion must exit 0, not ${result.code}: ${result.stdout}`);
  } else {
    assert.equal(kind, KIND_TIMEOUT,
      `non-zero steer must be ${KIND_COMPLETE} (non-processed) or ${KIND_TIMEOUT}: ${result.stdout}`);
  }
  assert.match(`${result.stderr}\n${result.stdout}`, TOLERATED_TERMINAL_STEM,
    `non-zero steer exit ${result.code} lacked a known terminal stem; stdout=${result.stdout} stderr=${result.stderr}`);
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

      const installed = await installPlatformOpsFleet({ env, seedFixtureSecrets: false });
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

    it("steer <id> <message> --json is accepted and emits a structured envelope", async () => {
      assert.ok(fleetId, "fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, ONE_SHOT_MESSAGE, JSON_FLAG]);
      assertSteerAccepted(result);
    }, STEER_TIMEOUT_MS);

    it("steer <id> with a whitespace-only message is rejected client-side", async () => {
      assert.ok(fleetId, "fleet was not installed in beforeAll");
      const result = await runWithEnv([STEER_COMMAND, fleetId, WHITESPACE_MESSAGE, JSON_FLAG]);
      assert.notEqual(result.code, 0, `expected non-zero; stdout=${result.stdout} stderr=${result.stderr}`);
      assert.match(`${result.stderr}\n${result.stdout}`, /message is required/i,
        `expected "message is required" stem; got stdout=${result.stdout} stderr=${result.stderr}`);
    });

    it("steer with no <fleet_id> exits non-zero with a usage stem", async () => {
      const result = await runWithEnv([STEER_COMMAND, JSON_FLAG]);
      assert.notEqual(result.code, 0, `expected non-zero; stdout=${result.stdout} stderr=${result.stderr}`);
      assert.match(`${result.stderr}\n${result.stdout}`.toLowerCase(), /missing|required|usage|expected/,
        `expected a missing-arg stem; got stdout=${result.stdout} stderr=${result.stderr}`);
    });
  });
}
