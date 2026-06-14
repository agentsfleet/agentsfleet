// In-process integration tests for `memory list|search` — runCli against a
// loopback mock API. Human-table paths mark the buffer stream isTTY=true
// (a subprocess pipe can never be a terminal, so the table tier lives
// here); the piped/auto-JSON path uses a plain buffer (isTTY undefined),
// which is exactly what a pipe looks like.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir, type TestStream } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-0000005e4e71";
const ZOMBIE_ID = "01900000-0000-7000-8000-0000005e4e72";
const MEMORIES_ROUTE = `GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/memories`;
const MEMORIES_PATH = `/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/memories`;

// The wire shape: numeric epoch milliseconds (schema/013 BIGINT).
const ITEMS = [
  { key: "acme_contact", content: "ops@acme.test is the escalation contact", category: "core", updated_at: 1765500300000 },
  { key: "deploy_window", content: "deploys freeze on fridays", category: "daily", updated_at: 1765500200000 },
];

const ENVELOPE = { items: ITEMS, total: ITEMS.length, request_id: "req_mem_int" };

const ttyStream = (): { stream: TestStream; read: () => string } => {
  const buf = bufferStream();
  buf.stream.isTTY = true;
  return buf;
};

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_memory" }, fn);

describe("memory list — human table on a terminal", () => {
  test("test_memory_list_table_newest_first: keys, categories, ISO timestamps, previews", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("KEY");
        expect(text).toContain("acme_contact");
        expect(text).toContain("deploy_window");
        expect(text).toContain("core");
        expect(text).toMatch(/\d{4}-\d{2}-\d{2}T/);
        expect(text).toContain("escalation contact");
        // server order preserved: newest fixture renders first
        expect(text.indexOf("acme_contact")).toBeLessThan(text.indexOf("deploy_window"));
        // no operator limit → none forwarded; the server defaults apply
        expect(calls[0]?.search ?? "").not.toContain("limit=");
        expect(calls[0]?.path).toBe(MEMORIES_PATH);
        expect(calls[0]?.method).toBe("GET");
      });
    });
  });

  test("test_memory_list_flags_and_validation: --category and --limit forward on the wire", async () => {
    // The invalid-value half of this dimension (`--limit 0` → usage error,
    // zero requests) lives at the subprocess tier (acceptance/memory-read
    // .spec.ts): commander's exitOverride applies to the root command only,
    // so an invalid option value on a subcommand process.exit()s an
    // in-process runCli — the same reason json-contract.test.ts carries a
    // recursive silenceTree().
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, { items: [], total: 0, request_id: "r" }) };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = ttyStream();
        const err = bufferStream();
        const okCode = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID, "--category", "daily", "--limit", "5"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(okCode).toBe(0);
        expect(calls[0]?.search).toContain("category=daily");
        expect(calls[0]?.search).toContain("limit=5");
      });
    });
  });

  test("test_memory_list_empty_exit_zero: friendly line + hygiene pointer, exit 0", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, { items: [], total: 0, request_id: "r" }) };
      await withMockApi(routes, async (apiUrl) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no memories stored/i);
        expect(out.read()).toContain("docs.agentsfleet.net/memory");
      });
    });
  });
});

describe("memory search", () => {
  test("test_memory_search_matches: query forwarded, matching rows rendered", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [MEMORIES_ROUTE]: () => jsonResponse(200, { items: [ITEMS[0]], total: 1, request_id: "r" }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "search", "--zombie", ZOMBIE_ID, "acme"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(calls[0]?.search).toContain("query=acme");
        const text = out.read();
        expect(text).toContain("acme_contact");
        expect(text).not.toContain("deploy_window");
      });
    });
  });

  test("test_memory_search_empty_exit_zero: no-match message + docs pointer, exit 0", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, { items: [], total: 0, request_id: "r" }) };
      await withMockApi(routes, async (apiUrl) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "search", "--zombie", ZOMBIE_ID, "ghost"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/no memories matched "ghost"/i);
        expect(out.read()).toContain("docs.agentsfleet.net/memory");
      });
    });
  });
});

describe("memory — machine-stable JSON", () => {
  test("test_memory_output_json_when_piped: non-TTY stdout emits the strict envelope, content byte-identical", async () => {
    // 16 KiB-class multibyte content — the preview cap must not touch JSON.
    const bigContent = `naïve café 🦉 ${"統合テスト".repeat(800)}`;
    const envelope = {
      items: [{ key: "big_note", content: bigContent, category: "core", updated_at: 1765500300000 }],
      total: 1,
      request_id: "req_mem_json",
    };
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, envelope) };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream(); // isTTY undefined — a pipe
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as typeof envelope;
        expect(parsed.items[0]?.content).toBe(bigContent);
        expect(parsed.items[0]?.updated_at).toBe(1765500300000); // raw passthrough
        expect(parsed.request_id).toBe("req_mem_json");
        expect(parsed.total).toBe(1);
      });
    });
  });

  test("--json forces the envelope even on a terminal", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = { [MEMORIES_ROUTE]: () => jsonResponse(200, ENVELOPE) };
      await withMockApi(routes, async (apiUrl) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID, "--json"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as typeof ENVELOPE;
        expect(parsed.items).toHaveLength(2);
      });
    });
  });
});

describe("memory — error shapes", () => {
  test("test_memory_unknown_zombie_error_shape: UZ-MEM-002 + zombie-listing suggestion, nonzero exit", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [MEMORIES_ROUTE]: () =>
          jsonResponse(404, {
            error: { code: "UZ-MEM-002", message: "zombie not found" },
            request_id: "req_mem_404",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        const stderrText = err.read();
        expect(stderrText).toContain("UZ-MEM-002");
        expect(stderrText).toContain("agentsfleet list");
        expect(stderrText).toContain("request_id: req_mem_404");
      });
    });
  });

  test("UZ-MEM-003 (backend unavailable) carries the retry suggestion, nonzero exit", async () => {
    await authedScope(async () => {
      // 500, not 503 — 503 is in the transport's RETRYABLE_STATUSES and
      // would burn ~750ms of backoff; the remap under test keys on the
      // UZ-MEM-003 code, not the status.
      const routes: MockRoutes = {
        [MEMORIES_ROUTE]: () =>
          jsonResponse(500, {
            error: { code: "UZ-MEM-003", message: "memory backend unavailable" },
            request_id: "req_mem_503",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = ttyStream();
        const err = bufferStream();
        const code = await runCli(
          ["memory", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-MEM-003");
        expect(err.read()).toMatch(/retry shortly/);
      });
    });
  });

  test("missing auth: fresh state dir → existing auth-error path with login suggestion, nonzero exit", async () => {
    await withFreshStateDir(async () => {
      const out = ttyStream();
      const err = bufferStream();
      const code = await runCli(
        ["memory", "list", "--zombie", ZOMBIE_ID],
        { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: "http://127.0.0.1:9" } },
      );
      expect(code).not.toBe(0);
      expect(err.read()).toMatch(/login/);
    });
  });

  test("test_memory_network_error_shape: unreachable endpoint → structured error with suggestion, nonzero exit, no partial table", async () => {
    await authedScope(async () => {
      const out = ttyStream();
      const err = bufferStream();
      const code = await runCli(
        ["memory", "list", "--zombie", ZOMBIE_ID],
        // loopback discard port — nothing listens, refuses instantly
        { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: "http://127.0.0.1:9" } },
      );
      expect(code).not.toBe(0);
      const stderrText = err.read();
      expect(stderrText).toContain("Suggestion:");
      // a NetworkError, not a server-code failure misclassified as one
      expect(stderrText).toMatch(/reach|connect|network/i);
      expect(stderrText).not.toMatch(/UZ-MEM-/);
      expect(out.read()).not.toContain("KEY");
    });
  });
});
