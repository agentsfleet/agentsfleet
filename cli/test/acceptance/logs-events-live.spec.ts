/**
 * logs-events-live — live `logs` + `events` read paths against a freshly
 * installed agent (token-injected, mirrors lifecycle-with-token.spec.ts).
 *
 * Scenario:
 *   - mint a Clerk session JWT via the admin path
 *   - hydrate workspaces.json directly from the API (the CLI only
 *     hydrates inside the login flow)
 *   - install the platform-ops bundle (prefix-scoped name) — the install
 *     itself generates the first events on the agent's timeline
 *   - `logs --agent <id> --json` returns a bounded, parseable
 *     `{items, next_cursor}` envelope; the read is time-bounded via
 *     `runAgentctl`'s `timeoutMs` so a wedged backend can't hang the suite
 *   - `events <id> --json` cursor walk: page across the paginator until it
 *     stops returning `next_cursor`, asserting (a) no infinite loop — the
 *     walk is capped at MAX_PAGES, (b) cursor monotonicity — a re-emitted
 *     cursor aborts the walk, (c) every page is exit-0 with a parseable
 *     `{items, next_cursor}` envelope and a finite numeric `created_at` on
 *     every row that carries one. The CLI streams the server's row order
 *     through untouched (`printJson(res)`) and promises NO ascending or
 *     descending direction, so the per-row check asserts only that
 *     timestamps are well-formed, never their direction — a directional
 *     assertion would flake the moment the timeline is served newest-first.
 *
 * Negative paths (no network residue / structured errors):
 *   - `events` with a missing `<agent_id>` rejected by commander
 *   - `logs --limit` out of bounds rejected client-side (EVENTS_LIMIT_BOUNDS)
 *
 * Teardown: prefix-scoped `cleanWorkspaceAgents` — only this run's agents
 * are killed; shared-tenant residue from other runs is left untouched and
 * global emptiness is never asserted.
 *
 * The minted JWT must not appear in any spawn's stdout/stderr
 * (`assertNoSecretLeak` after every `runAgentctl`).
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
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, expectMissingArg } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import {
  AGENT_FLAG,
  CURSOR_FLAG,
  EVENTS_COMMAND,
  ITEMS_KEY,
  JSON_FLAG,
  LIMIT_FLAG,
  LOGS_COMMAND,
  MAX_PAGES,
  NEXT_CURSOR_KEY,
  walkEventsCursor,
} from "./fixtures/logs-events-ops.ts";
import type { EventItem, EventsEnvelope } from "./fixtures/logs-events-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// Wire/output literals (RULE UFS — each used >=2x or crosses a boundary).
const STATE_DIR_PREFIX = "agentsfleet-logs-events-" as const;
const TOKEN_ENV_KEY = "AGENTSFLEET_TOKEN" as const;
const API_URL_ENV_KEY = "AGENTSFLEET_API_URL" as const;
const STATE_DIR_ENV_KEY = "AGENTSFLEET_STATE_DIR" as const;
const NO_COLOR_ENV_KEY = "NO_COLOR" as const;
const NO_COLOR_ON = "1" as const;

// `logs` is a single bounded HTTP read (not a follow/stream), but cap it
// anyway so a wedged backend surfaces as a `runAgentctl` timeout rather
// than hanging the suite. Generous enough for a cold DEV agent's first page.
const LOGS_READ_TIMEOUT_MS = 30_000;

// Out-of-bounds limit: EVENTS_LIMIT_BOUNDS is {min:1, max:500}, so 0 trips
// the client-side `parseIntOption` validator before any network call.
const OUT_OF_BOUNDS_LIMIT = "0" as const;

const CREATED_AT_KEY = "created_at" as const;

// Per-page `--limit` used by the supplied-cursor round-trip (>=2x → UFS).
const CURSOR_PROBE_LIMIT = "2" as const;

// Cursor walks fan out a page per round-trip; give them generous live
// budgets so a cold DEV agent doesn't trip the default test timeout.
const WALK_TIMEOUT_MS = 60_000;
const CURSOR_ROUNDTRIP_TIMEOUT_MS = 30_000;

if (!isLive) {
  describe("logs-events-live.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("logs-events-live — logs tail + events cursor walk", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let agentId = "";

    async function runWithEnv(
      args: ReadonlyArray<string>,
      timeoutMs?: number,
    ): Promise<RunResult> {
      const result = await runAgentctl(args, timeoutMs === undefined ? { env } : { env, timeoutMs });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        [TOKEN_ENV_KEY]: sessionJwt,
        [API_URL_ENV_KEY]: apiUrl,
        [STATE_DIR_ENV_KEY]: stateDir,
        [NO_COLOR_ENV_KEY]: NO_COLOR_ON,
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;

      // Install once for the whole suite — generates the timeline the logs
      // tail and the events walk both read. Prefix-scoped name so teardown
      // only reaps this run's agent.
      const installed = await installPlatformOpsAgent({ env });
      const id = installed.id ?? installed.agent_id;
      if (!id) throw new Error(`install missing id: ${JSON.stringify(installed)}`);
      agentId = id;
    });

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown — shared DEV tenant */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    describe("logs tail (bounded)", () => {
      it("logs --agent <id> --json returns a parseable {items,next_cursor} envelope", async () => {
        const result = await runWithEnv(
          [LOGS_COMMAND, AGENT_FLAG, agentId, JSON_FLAG],
          LOGS_READ_TIMEOUT_MS,
        );
        assert.equal(result.code, 0, `logs exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim() || "{}") as Record<string, unknown>;
        assert.equal(typeof parsed, "object", `logs payload not an object: ${result.stdout}`);
        // `items` may be absent on a brand-new agent, but when present it
        // must be an array; `next_cursor`, when present, is a string|null.
        if (ITEMS_KEY in parsed) {
          assert.ok(Array.isArray(parsed[ITEMS_KEY]), `logs ${ITEMS_KEY} not an array: ${result.stdout}`);
        }
        if (NEXT_CURSOR_KEY in parsed) {
          const nc = parsed[NEXT_CURSOR_KEY];
          assert.ok(nc === null || typeof nc === "string", `logs ${NEXT_CURSOR_KEY} not string|null`);
        }
      });

      it("logs honours --limit and never overruns the cap", async () => {
        const limit = 3;
        const result = await runWithEnv(
          [LOGS_COMMAND, AGENT_FLAG, agentId, LIMIT_FLAG, String(limit), JSON_FLAG],
          LOGS_READ_TIMEOUT_MS,
        );
        assert.equal(result.code, 0, `logs --limit exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
        const items = Array.isArray(parsed.items) ? parsed.items : [];
        assert.ok(items.length <= limit, `logs returned ${items.length} items, exceeds --limit ${limit}`);
      });
    });

    describe("events cursor walk", () => {
      it("walks pages to exhaustion without an infinite loop", async () => {
        const walk = await walkEventsCursor(env, agentId, (page, idx) => {
          assertEnvelopeShape(page, idx);
          assertTimestampsWellFormed(page, idx);
        });
        // The walk either exhausted the paginator (server stopped sending
        // next_cursor) or hit the page cap. Hitting the cap means the
        // server never terminated pagination on a small fixture — that's a
        // failure, not a tolerated outcome.
        assert.ok(walk.exhausted, `events paginator never exhausted within ${MAX_PAGES} pages`);
        assert.ok(walk.pages >= 1, `expected >=1 page walked, got ${walk.pages}`);
        // Bound proof: pages walked never exceeds the hard cap.
        assert.ok(walk.pages <= MAX_PAGES, `walked ${walk.pages} pages, exceeds cap ${MAX_PAGES}`);
      }, WALK_TIMEOUT_MS);

      it("emitted cursors are strictly distinct (monotonic, no cycles)", async () => {
        const walk = await walkEventsCursor(env, agentId);
        const unique = new Set(walk.cursors);
        assert.equal(unique.size, walk.cursors.length,
          `cursor sequence had duplicates: ${JSON.stringify(walk.cursors)}`);
        // Every cursor we sent on a follow-up page is a real string token.
        for (const cursor of walk.cursors) {
          assert.equal(typeof cursor, "string");
          assert.ok(cursor.length > 0, "empty cursor token returned by paginator");
        }
      }, WALK_TIMEOUT_MS);

      it("a supplied cursor is accepted and re-paginates deterministically", async () => {
        const first = await runWithEnv(
          [EVENTS_COMMAND, agentId, LIMIT_FLAG, CURSOR_PROBE_LIMIT, JSON_FLAG],
          CURSOR_ROUNDTRIP_TIMEOUT_MS,
        );
        assert.equal(first.code, 0, `events first page exited ${first.code}: ${first.stderr}`);
        const firstParsed = JSON.parse(first.stdout.trim() || "{}") as { next_cursor?: unknown };
        const cursor = typeof firstParsed.next_cursor === "string" ? firstParsed.next_cursor : null;
        if (!cursor) return; // single-page fixture — nothing further to assert.
        const second = await runWithEnv(
          [EVENTS_COMMAND, agentId, LIMIT_FLAG, CURSOR_PROBE_LIMIT, CURSOR_FLAG, cursor, JSON_FLAG],
          CURSOR_ROUNDTRIP_TIMEOUT_MS,
        );
        assert.equal(second.code, 0, `events --cursor exited ${second.code}: ${second.stderr}`);
        const secondParsed = JSON.parse(second.stdout.trim() || "{}") as Record<string, unknown>;
        assert.ok(ITEMS_KEY in secondParsed, `cursor page missing ${ITEMS_KEY}: ${second.stdout}`);
        assert.ok(Array.isArray(secondParsed[ITEMS_KEY]), `cursor page ${ITEMS_KEY} not an array`);
      }, CURSOR_ROUNDTRIP_TIMEOUT_MS);
    });

    describe("negative paths (no residue)", () => {
      it("events with no <agent_id> is rejected by commander", async () => {
        const result = await expectMissingArg([EVENTS_COMMAND], env);
        assertNoSecretLeak(result, sessionJwt);
      });

      it("logs --limit out of bounds is rejected client-side", async () => {
        const result = await runWithEnv(
          [LOGS_COMMAND, AGENT_FLAG, agentId, LIMIT_FLAG, OUT_OF_BOUNDS_LIMIT, JSON_FLAG],
        );
        assert.notEqual(result.code, 0,
          `expected non-zero for out-of-bounds --limit; stdout=${result.stdout} stderr=${result.stderr}`);
      });
    });
  });
}

/**
 * Per-page envelope shape: `{items: array, next_cursor: string|null}`.
 * `fetchEventsPage` already guarantees exit-0 and JSON parse — this
 * tightens the structural contract every page must satisfy.
 */
function assertEnvelopeShape(page: EventsEnvelope, pageIndex: number): void {
  assert.ok(Array.isArray(page.items), `page ${pageIndex}: ${ITEMS_KEY} not an array`);
  const nc = page.nextCursor;
  assert.ok(nc === null || typeof nc === "string", `page ${pageIndex}: ${NEXT_CURSOR_KEY} not string|null`);
}

/**
 * Every `created_at` a row carries must be a finite number (epoch ms — the
 * shape `agent_events.ts` feeds to `new Date(ev.created_at)`). The CLI
 * passes the server's row order through verbatim and promises no
 * ascending/descending direction, so this asserts well-formedness only,
 * never ordering — a directional check would flake on a newest-first
 * timeline. Rows without a `created_at` are tolerated (not every event
 * row carries one) but a present-yet-non-finite value is a contract break.
 */
function assertTimestampsWellFormed(page: EventsEnvelope, pageIndex: number): void {
  for (const item of page.items as ReadonlyArray<EventItem>) {
    const raw = item[CREATED_AT_KEY];
    if (raw === undefined || raw === null) continue;
    assert.ok(typeof raw === "number" && Number.isFinite(raw),
      `page ${pageIndex}: ${CREATED_AT_KEY} present but not a finite number: ${JSON.stringify(raw)}`);
  }
}
