/**
 * Subprocess e2e for `memory list|search` against the BUILT binary
 * (dist/bin/agentsfleet.js) — creds-free: a stubbed state dir satisfies the
 * auth/workspace guards and a loopback mock serves the memories endpoint.
 *
 * A spawned child's stdout is a pipe, never a TTY — so every happy-path
 * run here doubles as the auto-JSON-when-piped proof (7 Pillars): the
 * binary must emit the strict envelope WITHOUT --json. The human-table
 * rendering is covered at the in-process tier (memory.integration.test.ts)
 * where the test stream can be marked isTTY.
 */

import { describe, it } from "bun:test";
import assert from "node:assert/strict";

import { runZombiectl, composeEnv } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { makeStubbedStateDir } from "./fixtures/state-dir.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "../helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-0000005e4e71";
const ZOMBIE_ID = "01900000-0000-7000-8000-0000005e4e72";
const MEMORIES_ROUTE = `GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/memories`;

// The wire shape: numeric epoch milliseconds (schema/013 BIGINT).
const ENVELOPE = {
  items: [
    { key: "acme_contact", content: "ops@acme.test is the escalation contact", category: "core", updated_at: 1765500300000 },
  ],
  total: 1,
  request_id: "req_mem_e2e",
};

interface Envelope {
  items: Array<{ key: string; content: string; category: string; updated_at: number }>;
  total: number;
  request_id: string;
}

const helpEnv = (): Record<string, string> =>
  composeEnv({ ZOMBIE_API_URL: UNROUTABLE_API_URL, NO_COLOR: "1" });

async function withStubbedRun<T>(
  routes: MockRoutes,
  fn: (run: (args: string[]) => Promise<{ code: number | null; stdout: string; stderr: string }>, calls: import("../helpers-mock-api.ts").MockCall[]) => Promise<T>,
): Promise<T> {
  const stub = await makeStubbedStateDir({ workspaceId: WS_ID });
  try {
    return await withMockApi(routes, async (apiUrl, calls) => {
      const env = composeEnv({
        ZOMBIE_API_URL: apiUrl,
        ZOMBIE_STATE_DIR: stub.dir,
        NO_COLOR: "1",
      });
      return fn((args) => runZombiectl(args, { env }), calls);
    });
  } finally {
    await stub.cleanup();
  }
}

describe("test_memory_help_e2e — built binary renders the documented grammar", () => {
  it("`memory --help` lists both verbs", async () => {
    const result = await runZombiectl(["memory", "--help"], { env: helpEnv() });
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /list/);
    assert.match(result.stdout, /search \[options\] <query>/);
    assert.match(result.stdout, /read-only/i);
  });

  it("`memory list --help` documents --zombie/--category/--limit/--workspace", async () => {
    const result = await runZombiectl(["memory", "list", "--help"], { env: helpEnv() });
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /--zombie <id>/);
    assert.match(result.stdout, /--category <name>/);
    assert.match(result.stdout, /--limit <n>/);
    assert.match(result.stdout, /--workspace <id>/);
  });

  it("`memory search --help` documents the positional query and carries no --category", async () => {
    const result = await runZombiectl(["memory", "search", "--help"], { env: helpEnv() });
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /search.*<query>/);
    assert.match(result.stdout, /--zombie <id>/);
    assert.doesNotMatch(result.stdout, /--category/);
  });
});

describe("test_memory_e2e_list_search — subprocess against a stubbed endpoint", () => {
  it("piped `memory list` emits the strict envelope without --json (auto-JSON pillar)", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "list", "--zombie", ZOMBIE_ID]);
      assert.equal(result.code, 0, result.stderr);
      const parsed = JSON.parse(result.stdout) as Envelope;
      assert.equal(parsed.items[0]?.key, "acme_contact");
      assert.equal(parsed.items[0]?.updated_at, 1765500300000);
      assert.equal(parsed.request_id, "req_mem_e2e");
      assert.equal(calls[0]?.method, "GET");
      assert.ok(!(calls[0]?.search ?? "").includes("limit="), "no operator limit → none forwarded");
    });
  });

  it("`memory search` forwards the query and renders JSON when piped", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "search", "--zombie", ZOMBIE_ID, "acme"]);
      assert.equal(result.code, 0, result.stderr);
      const parsed = JSON.parse(result.stdout) as Envelope;
      assert.equal(parsed.total, 1);
      assert.ok((calls[0]?.search ?? "").includes("query=acme"));
    });
  });

  it("empty store exits 0 with a parseable empty envelope (empty ≠ error)", async () => {
    await withStubbedRun(
      { [MEMORIES_ROUTE]: () => jsonResponse(200, { items: [], total: 0, request_id: "req_mem_empty" }) },
      async (run) => {
        const result = await run(["memory", "list", "--zombie", ZOMBIE_ID]);
        assert.equal(result.code, 0, result.stderr);
        const parsed = JSON.parse(result.stdout) as Envelope;
        assert.deepEqual(parsed.items, []);
      },
    );
  });

  it("unknown zombie: UZ-MEM-002 + zombie-listing suggestion on stderr, nonzero exit", async () => {
    await withStubbedRun(
      {
        [MEMORIES_ROUTE]: () =>
          jsonResponse(404, { error: { code: "UZ-MEM-002", message: "zombie not found" }, request_id: "req_mem_404" }),
      },
      async (run) => {
        const result = await run(["memory", "list", "--zombie", ZOMBIE_ID]);
        assert.notEqual(result.code, 0);
        assert.match(result.stderr, /UZ-MEM-002/);
        assert.match(result.stderr, /agentsfleet list/);
      },
    );
  });

  it("`--limit 0` is rejected client-side before any request", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "list", "--zombie", ZOMBIE_ID, "--limit", "0"]);
      assert.notEqual(result.code, 0);
      assert.match(result.stderr, /must be/);
      assert.equal(calls.length, 0, "invalid limit must not reach the API");
    });
  });

  it("`--limit 101` exceeds the mirrored server cap and never reaches the API", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "list", "--zombie", ZOMBIE_ID, "--limit", "101"]);
      assert.notEqual(result.code, 0);
      assert.match(result.stderr, /must be ≤ 100/);
      assert.equal(calls.length, 0, "over-cap limit must not reach the API");
    });
  });

  it("a malformed --zombie id is rejected client-side as uuidv7 before any request", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "list", "--zombie", "not-a-uuid"]);
      assert.notEqual(result.code, 0);
      assert.match(result.stderr, /expected uuidv7 format/);
      assert.equal(calls.length, 0, "malformed id must not reach the API");
    });
  });

  it("`memory search` without a query is rejected by commander before any request", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "search", "--zombie", ZOMBIE_ID]);
      assert.notEqual(result.code, 0);
      assert.match(result.stderr, /missing|required/i);
      assert.equal(calls.length, 0);
    });
  });

  it("bare `memory list` fails with the --zombie usage suggestion through the real pipeline", async () => {
    await withStubbedRun({ [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) }, async (run, calls) => {
      const result = await run(["memory", "list"]);
      assert.notEqual(result.code, 0);
      assert.match(result.stderr, /--zombie <id> is required/);
      assert.equal(calls.length, 0);
    });
  });
});
