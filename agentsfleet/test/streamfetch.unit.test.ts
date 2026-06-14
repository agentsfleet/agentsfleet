import { describe, test, expect } from "bun:test";
import { streamFetch, type SseEvent } from "../src/lib/stream-fetch.ts";
import { ApiError } from "../src/lib/http.ts";
import { asFetchImpl, type ResponseLike } from "./helpers.ts";

// ── Test helpers ─────────────────────────────────────────────────────────────

interface FakeReader {
  read(): Promise<{ done: true } | { done: false; value: Uint8Array }>;
}
interface FakeStream { getReader(): FakeReader }
type FakeResponse = ResponseLike & { body?: FakeStream | null };

function sseResponseFrom(sseBody: string, status = 200): FakeResponse {
  const encoder = new TextEncoder();
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "Error",
    headers: { get: () => null },
    text: async () => sseBody,
    body: {
      getReader() {
        let sent = false;
        return {
          read(): Promise<{ done: true } | { done: false; value: Uint8Array }> {
            if (!sent) {
              sent = true;
              return Promise.resolve({ done: false, value: encoder.encode(sseBody) });
            }
            return Promise.resolve({ done: true });
          },
        };
      },
    },
  };
}

// ── T1: Happy path ──────────────────────────────────────────────────────────

describe("streamFetch — happy path", () => {
  test("parses SSE events and calls onEvent for each", async () => {
    const events: SseEvent[] = [];
    const fetchImpl = asFetchImpl(async () => sseResponseFrom(
      'event: tool_use\ndata: {"id":"tu_01","name":"read_file"}\n\nevent: done\ndata: {"ok":true}\n\n',
    ));
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(2);
    expect(events[0]?.type).toBe("tool_use");
    expect((events[0]?.data as { name?: string } | undefined)?.name).toBe("read_file");
    expect(events[1]?.type).toBe("done");
  });

  test("merges custom headers with content-type and accept", async () => {
    // Boxed in {} so the closure assignment widens the read-side narrowing
    // (assigning a `let` inside a Promise callback leaves the post-await read
    // narrowed to its initial type under strict control-flow analysis).
    const captured: { headers: Record<string, string> | null } = { headers: null };
    const fetchImpl = asFetchImpl(async (_url, opts) => {
      captured.headers = (opts?.headers ?? null) as Record<string, string> | null;
      return sseResponseFrom('event: done\ndata: {}\n\n');
    });
    await streamFetch("https://api.test.com/v1/x", { msg: 1 },
      { Authorization: "Bearer tok" }, () => {}, { fetchImpl });
    expect(captured.headers?.Authorization).toBe("Bearer tok");
    expect(captured.headers?.["Content-Type"]).toBe("application/json");
    expect(captured.headers?.Accept).toBe("text/event-stream");
  });

  test("sends JSON-stringified payload as body", async () => {
    let capturedBody: string | null = null;
    const fetchImpl = asFetchImpl(async (_url, opts) => {
      capturedBody = (opts?.body as string | undefined) ?? null;
      return sseResponseFrom('event: done\ndata: {}\n\n');
    });
    await streamFetch("https://api.test.com/v1/x", { messages: ["hello"], tools: [] },
      {}, () => {}, { fetchImpl });
    const parsed = JSON.parse(capturedBody ?? "{}") as { messages?: string[] };
    expect(parsed.messages).toEqual(["hello"]);
  });
});

// ── T2: Edge cases ──────────────────────────────────────────────────────────

describe("streamFetch — edge cases", () => {
  test("handles empty SSE body (no events)", async () => {
    const events: SseEvent[] = [];
    const fetchImpl = asFetchImpl(async () => sseResponseFrom(""));
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(0);
  });

  test("skips heartbeat comments", async () => {
    const events: SseEvent[] = [];
    const fetchImpl = asFetchImpl(async () => sseResponseFrom(': heartbeat\n\nevent: done\ndata: {"ok":true}\n\n'));
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(1);
    expect(events[0]?.type).toBe("done");
  });

  test("handles multi-chunk delivery (split across reads)", async () => {
    const encoder = new TextEncoder();
    const chunk1 = 'event: text_delta\ndata: {"te';
    const chunk2 = 'xt":"hello"}\n\nevent: done\ndata: {}\n\n';
    let readCount = 0;
    const fetchImpl = asFetchImpl(async () => ({
      ok: true,
      status: 200,
      statusText: "OK",
      headers: { get: () => null },
      text: async () => "",
      body: {
        getReader() {
          return {
            read(): Promise<{ done: true } | { done: false; value: Uint8Array }> {
              readCount++;
              if (readCount === 1) return Promise.resolve({ done: false, value: encoder.encode(chunk1) });
              if (readCount === 2) return Promise.resolve({ done: false, value: encoder.encode(chunk2) });
              return Promise.resolve({ done: true });
            },
          };
        },
      },
    } as FakeResponse));
    const events: SseEvent[] = [];
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(2);
    expect((events[0]?.data as { text?: string } | undefined)?.text).toBe("hello");
  });
});

// ── T3: Error paths ─────────────────────────────────────────────────────────

describe("streamFetch — error paths", () => {
  test("non-200 response throws ApiError with parsed error code", async () => {
    const fetchImpl = asFetchImpl(async () => ({
      ok: false,
      status: 403,
      statusText: "Forbidden",
      headers: { get: () => null },
      text: async () => JSON.stringify({ error: { code: "UZ-AUTH-001", message: "access denied" } }),
    }));
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect(err).toBeInstanceOf(ApiError);
      const apiErr = err as ApiError;
      expect(apiErr.status).toBe(403);
      expect(apiErr.code).toBe("UZ-AUTH-001");
    }
  });

  test("non-200 with non-JSON body still throws ApiError", async () => {
    const fetchImpl = asFetchImpl(async () => ({
      ok: false,
      status: 502,
      statusText: "Bad Gateway",
      headers: { get: () => null },
      text: async () => "upstream error",
    }));
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).code).toBe("HTTP_502");
    }
  });

  test("timeout throws ApiError with TIMEOUT code", async () => {
    const fetchImpl = asFetchImpl(async (_url, opts) => {
      // Simulate slow response — wait longer than timeoutMs
      await new Promise((_resolve, reject) => {
        opts?.signal?.addEventListener("abort", () => {
          reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
        });
      });
      // unreachable — the abort listener rejects above. Satisfy the
      // FetchImpl return-type contract.
      return sseResponseFrom("");
    });
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, {
        fetchImpl,
        timeoutMs: 50,
      });
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect(err).toBeInstanceOf(ApiError);
      expect((err as ApiError).code).toBe("TIMEOUT");
    }
  });

  test("read error mid-stream propagates", async () => {
    const fetchImpl = asFetchImpl(async () => ({
      ok: true,
      status: 200,
      statusText: "OK",
      headers: { get: () => null },
      text: async () => "",
      body: {
        getReader() {
          return {
            read(): Promise<never> { return Promise.reject(new Error("socket hang up")); },
          };
        },
      },
    } as FakeResponse));
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err: unknown) {
      expect(err).toBeInstanceOf(Error);
      expect((err as Error).message).toContain("socket hang up");
    }
  });
});

// ── T4: Output fidelity — SSE parsing correctness ───────────────────────────

describe("streamFetch — SSE parsing fidelity", () => {
  test("parses event with JSON data containing nested objects", async () => {
    const events: SseEvent[] = [];
    const fetchImpl = asFetchImpl(async () => sseResponseFrom(
      'event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"src/main.go"}}\n\n',
    ));
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    const data = events[0]?.data as { input?: { path?: string } } | undefined;
    expect(data?.input?.path).toBe("src/main.go");
  });

  test("handles data with unicode content", async () => {
    const events: SseEvent[] = [];
    const fetchImpl = asFetchImpl(async () => sseResponseFrom(
      'event: text_delta\ndata: {"text":"中文テスト 👨‍💻"}\n\n',
    ));
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    const data = events[0]?.data as { text?: string } | undefined;
    expect(data?.text).toBe("中文テスト 👨‍💻");
  });
});
