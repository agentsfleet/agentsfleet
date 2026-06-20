/**
 * Performance-budget acceptance scenario (live, seeded-credentials session).
 *
 * This is a regression guardrail, NOT a microbenchmark. It measures the
 * end-to-end wall-clock of read-only CLI commands (`list --json`,
 * `doctor --json`) over a small fixed number of runs and asserts the
 * worst observed sample stays under a deliberately GENEROUS budget. The
 * goal is to catch pathological regressions (a command that suddenly
 * makes N serial round-trips, or hangs on a retry loop) without flaking
 * on ordinary network jitter against the shared DEV tenant. The budget
 * rationale lives on PER_COMMAND_BUDGET_MS below.
 *
 * It also walks the `events` cursor pages on a freshly-seeded fleet to
 * prove the pagination read-path stays bounded per page.
 *
 * Identity / hydration mirror lifecycle-with-token.spec.ts exactly: mint a
 * Clerk session JWT via the admin path, hydrate workspaces.json directly
 * from the API (the CLI only hydrates inside the login flow), spawn the
 * real CLI authenticating off the seeded credentials, and scrub the JWT
 * from every capture.
 *
 * Live-only: the entire suite registers only when
 * `AGENTSFLEET_ACCEPTANCE_TARGET` is an https URL; otherwise every test is
 * skipped — matches the unit runner's local invariant.
 *
 * Mutating state (the seeded fleet for the events walk) is run-prefix
 * scoped via ACCEPTANCE_RUN_PREFIX and torn down in afterAll. No global
 * emptiness is ever asserted.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX } from "./fixtures/constants.ts";
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

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// Number of samples per measured command. Small to bound CI wall-clock:
// at the budget below the absolute worst case for one command is
// SAMPLE_COUNT * PER_COMMAND_BUDGET_MS, and the suite measures two
// commands plus a short cursor walk.
const SAMPLE_COUNT = 5;

// Generous per-command wall-clock budget. This is a network-inclusive
// round-trip ceiling against a shared remote tenant, so it is set an order
// of magnitude above the expected p50 (~0.5–2s) to absorb cold caches,
// TLS handshakes, and DEV-tenant contention. A breach means a real
// regression (extra serial round-trips, a retry/backoff loop, or a hang),
// not jitter.
const PER_COMMAND_BUDGET_MS = 10_000;

// The cursor walk does at most this many page fetches; bounds the test's
// own runtime and guards against an unterminated pagination loop.
const MAX_EVENT_PAGES = 3;

// Small page size so a freshly-seeded fleet (few events) still exercises
// the cursor mechanics rather than returning everything in page one.
const EVENT_PAGE_LIMIT = 2;

// Per-spawn timeout: PER_COMMAND_BUDGET_MS plus headroom so a budget
// breach surfaces as a recorded slow sample (a clean assertion failure)
// rather than a spawn TimeoutError that swallows the duration.
const SPAWN_TIMEOUT_MS = PER_COMMAND_BUDGET_MS + 5_000;

const FLAG_JSON = "--json" as const;
const FLAG_CURSOR = "--cursor" as const;
const FLAG_LIMIT = "--limit" as const;
const COMMAND_LIST = "list" as const;
const COMMAND_DOCTOR = "doctor" as const;
const COMMAND_EVENTS = "events" as const;
const KEY_ITEMS = "items" as const;
const KEY_CHECKS = "checks" as const;
const KEY_NEXT_CURSOR = "next_cursor" as const;

interface MeasuredCommand {
  readonly label: string;
  readonly args: ReadonlyArray<string>;
  // Top-level JSON key the success envelope must carry; doubles as a
  // proof the command actually produced a real response (not a partial
  // write truncated by a hang) before we trust its timing.
  readonly requiredKey: string;
  readonly isList?: boolean;
}

const MEASURED_COMMANDS: ReadonlyArray<MeasuredCommand> = [
  { label: "fleet list --json", args: [COMMAND_LIST, FLAG_JSON], requiredKey: KEY_ITEMS, isList: true },
  { label: "doctor --json", args: [COMMAND_DOCTOR, FLAG_JSON], requiredKey: KEY_CHECKS },
];

interface EventsPage {
  readonly durationMs: number;
  readonly nextCursor: string | null;
}

// Inclusive max of a sample set. Used as the budget yardstick: with only
// SAMPLE_COUNT runs, the max IS the most conservative tail estimate (a
// true p95 over 5 points degenerates to the max anyway), so asserting on
// the max is both honest and strictly stronger than asserting on p50.
function maxOf(samples: ReadonlyArray<number>): number {
  assert.ok(samples.length > 0, "maxOf requires at least one sample");
  return samples.reduce((hi, n) => (n > hi ? n : hi), samples[0] as number);
}

function p50Of(samples: ReadonlyArray<number>): number {
  assert.ok(samples.length > 0, "p50Of requires at least one sample");
  const sorted = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted[mid] as number;
}

function eventsArgs(fleetId: string, cursor: string | null): ReadonlyArray<string> {
  return [
    COMMAND_EVENTS,
    fleetId,
    FLAG_LIMIT,
    String(EVENT_PAGE_LIMIT),
    FLAG_JSON,
    ...(cursor ? [FLAG_CURSOR, cursor] : []),
  ];
}

if (!isLive) {
  describe("perf.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("perf — read-command wall-clock budget (seeded-credentials session)", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";

    async function runWithEnv(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs: SPAWN_TIMEOUT_MS });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    // One events page fetch: spawn, assert exit 0 + `items` array + budget,
    // and surface the next cursor (null when the server stops paging). Kept
    // module-adjacent so the cursor-walk test body stays under the length cap.
    async function fetchEventsPage(fleetId: string, cursor: string | null, pageNo: number): Promise<EventsPage> {
      const result = await runWithEnv(eventsArgs(fleetId, cursor));
      assert.equal(result.code, 0, `events page ${pageNo} exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as { items?: unknown; next_cursor?: unknown };
      assert.ok(
        Array.isArray(parsed[KEY_ITEMS]),
        `events page ${pageNo}: ${KEY_ITEMS} not an array in ${result.stdout}`,
      );
      assert.ok(
        result.durationMs <= PER_COMMAND_BUDGET_MS,
        `events page ${pageNo} budget breach: ${result.durationMs}ms > ${PER_COMMAND_BUDGET_MS}ms`,
      );
      const next = parsed[KEY_NEXT_CURSOR];
      const nextCursor = typeof next === "string" && next.length > 0 ? next : null;
      return { durationMs: result.durationMs, nextCursor };
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-perf-"));
      env = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceFleets(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown — never fail the suite on cleanup */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // Each measured read command: SAMPLE_COUNT serial runs, every run must
    // exit 0 with a well-formed envelope, and the worst sample must clear
    // the budget. Serial (not parallel) so the timing reflects a single
    // operator's experience and avoids self-inflicted contention.
    for (const cmd of MEASURED_COMMANDS) {
      it(`${cmd.label}: max of ${SAMPLE_COUNT} runs under ${PER_COMMAND_BUDGET_MS}ms`, async () => {
        const samples: number[] = [];
        for (let i = 0; i < SAMPLE_COUNT; i += 1) {
          const result = await runWithEnv(cmd.args);
          assert.equal(result.code, 0, `${cmd.label} run ${i + 1} exited ${result.code}: ${result.stderr}`);
          const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
          assert.ok(cmd.requiredKey in parsed, `${cmd.label} run ${i + 1}: missing ${cmd.requiredKey} in ${result.stdout}`);
          if (cmd.isList) {
            assert.ok(Array.isArray(parsed[cmd.requiredKey]), `${cmd.label} run ${i + 1}: ${cmd.requiredKey} not an array`);
          }
          // Wall-clock measured by the spawner (process start → close),
          // not a re-implemented timer — the runner is the source of truth.
          assert.ok(
            Number.isFinite(result.durationMs) && result.durationMs >= 0,
            `${cmd.label} run ${i + 1}: durationMs not a sane number: ${result.durationMs}`,
          );
          samples.push(result.durationMs);
        }
        const worst = maxOf(samples);
        const median = p50Of(samples);
        assert.ok(
          worst <= PER_COMMAND_BUDGET_MS,
          `${cmd.label} budget breach: max=${worst}ms p50=${median}ms ` +
          `budget=${PER_COMMAND_BUDGET_MS}ms samples=${JSON.stringify(samples)}`,
        );
      }, SPAWN_TIMEOUT_MS * SAMPLE_COUNT + 5_000);
    }

    // Cursor-walk throughput: seed one fleet, then page `events` with a
    // small limit. Each page fetch must exit 0, carry an `items` array,
    // and clear the per-command budget. The walk terminates when the
    // server stops returning `next_cursor` or after MAX_EVENT_PAGES —
    // whichever comes first — so a broken (never-terminating) cursor
    // contract surfaces as a bounded, asserted failure rather than a hang.
    describe("events cursor-walk throughput", () => {
      let fleetId = "";

      beforeAll(async () => {
        const installed = await installPlatformOpsFleet({ env });
        const id = installed.id ?? installed.fleet_id;
        if (!id) throw new Error(`install missing id: ${JSON.stringify(installed)}`);
        fleetId = id;
      });

      it(`walks <= ${MAX_EVENT_PAGES} pages, each page under ${PER_COMMAND_BUDGET_MS}ms`, async () => {
        const pageDurations: number[] = [];
        let cursor: string | null = null;
        let pages = 0;

        while (pages < MAX_EVENT_PAGES) {
          const page = await fetchEventsPage(fleetId, cursor, pages + 1);
          pageDurations.push(page.durationMs);
          pages += 1;
          // null next_cursor is the documented "no more pages" terminator.
          if (page.nextCursor === null) break;
          assert.notEqual(page.nextCursor, cursor, "events next_cursor did not advance — pagination would loop");
          cursor = page.nextCursor;
        }

        assert.ok(pages >= 1, "events cursor-walk made no page fetches");
        assert.ok(
          maxOf(pageDurations) <= PER_COMMAND_BUDGET_MS,
          `events worst page ${maxOf(pageDurations)}ms over budget ${PER_COMMAND_BUDGET_MS}ms; ` +
          `pages=${JSON.stringify(pageDurations)}`,
        );
      }, SPAWN_TIMEOUT_MS * MAX_EVENT_PAGES + 5_000);
    });
  });
}
