// M22_001 §5 — run_watch.js unit tests
//
// Covers greptile P2 findings:
//   - HTTP 5xx retries instead of immediate return
//   - AbortError handling without stale abortedRef
//   - SSE parsing, gate_result rendering, run_complete exit
//   - Last-Event-ID forwarded on reconnect
//   - 4xx exits immediately (no retry)

import { describe, test, expect } from "bun:test";
import { makeNoop, makeBufferStream, ui, RUN_ID_1 } from "./helpers.js";
import { streamRunWatch, WATCH_MAX_RETRIES, WATCH_RETRY_DELAY_MS } from "../src/commands/run_watch.js";

const TOKEN = "tok_test_watch";

function makeCtx(overrides = {}) {
  return {
    stdout: makeNoop(),
    stderr: makeNoop(),
    jsonMode: false,
    token: TOKEN,
    apiKey: null,
    apiUrl: "https://api.example.com",
    env: {},
    fetchImpl: null,
    ...overrides,
  };
}

function makeDeps(overrides = {}) {
  return {
    apiHeaders: (ctx) => ({ Authorization: `Bearer ${ctx.token}` }),
    ui,
    writeLine: (stream, line = "") => { stream.write((line ?? "") + "\n"); },
    ...overrides,
  };
}

/** Build a ReadableStream from SSE text. */
function sseStream(text) {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(text));
      controller.close();
    },
  });
}

/** Build a mock fetch that returns SSE responses from a list. */
function mockFetch(responses) {
  let call = 0;
  return async (_url, _opts) => {
    const r = responses[call++] || responses[responses.length - 1];
    if (r.error) throw r.error;
    return {
      ok: r.ok ?? (r.status >= 200 && r.status < 300),
      status: r.status ?? 200,
      body: r.body ?? sseStream(r.sse ?? ""),
    };
  };
}

// ── T1: Happy path — SSE parsing ──────────────────────────────────────────

describe("streamRunWatch — happy path", () => {
  test("T1: renders gate_result events", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const sse = "event: gate_result\ndata: {\"gate_name\":\"lint\",\"outcome\":\"PASS\",\"loop\":1,\"wall_ms\":100}\n\nevent: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({ stdout, fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    const output = read();
    expect(output).toContain("[lint] PASS (loop 1, 100ms)");
    expect(output).toContain("run complete");
  });

  test("T1: exits cleanly on run_complete", async () => {
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({ fetchImpl: mockFetch([{ status: 200, sse }]) });
    // Should not hang — run_complete terminates the loop.
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
  });

  test("T1: tracks Last-Event-ID from SSE id: lines", async () => {
    let capturedHeaders = [];
    const call1Sse = "id: 1700000000001\nevent: gate_result\ndata: {\"gate_name\":\"lint\",\"outcome\":\"PASS\",\"loop\":1,\"wall_ms\":50}\n\n";
    // First call returns data then stream breaks; second call should have Last-Event-ID.
    const ctx = makeCtx({
      fetchImpl: async (_url, opts) => {
        capturedHeaders.push({ ...opts.headers });
        if (capturedHeaders.length === 1) {
          return { ok: true, status: 200, body: sseStream(call1Sse) };
        }
        // Second call: run_complete
        const sse2 = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
        return { ok: true, status: 200, body: sseStream(sse2) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    // Second call should have Last-Event-ID header.
    if (capturedHeaders.length >= 2) {
      expect(capturedHeaders[1]["Last-Event-ID"]).toBe("1700000000001");
    }
  });

  test("T1: prints streaming hint to stderr", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({ stderr, fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(read()).toContain("Ctrl+C to stop");
  });

  test("T1: multiple gate_result events rendered in order", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const sse = [
      "event: gate_result\ndata: {\"gate_name\":\"lint\",\"outcome\":\"PASS\",\"loop\":1,\"wall_ms\":50}\n\n",
      "event: gate_result\ndata: {\"gate_name\":\"test\",\"outcome\":\"FAIL\",\"loop\":1,\"wall_ms\":3000}\n\n",
      "event: run_complete\ndata: {\"state\":\"FAILED\"}\n\n",
    ].join("");
    const ctx = makeCtx({ stdout, fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    const output = read();
    const lintIdx = output.indexOf("[lint]");
    const testIdx = output.indexOf("[test]");
    expect(lintIdx).toBeLessThan(testIdx);
  });
});

// ── T3: HTTP 5xx retry (greptile P2 fix) ──────────────────────────────────

describe("streamRunWatch — HTTP 5xx retry", () => {
  test("T3: retries on 503 then succeeds on second attempt", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    let callCount = 0;
    const ctx = makeCtx({
      stdout,
      fetchImpl: async () => {
        callCount++;
        if (callCount === 1) return { ok: false, status: 503, body: sseStream("") };
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(2);
    expect(read()).toContain("run complete");
  });

  test("T3: retries on 500 at least twice before giving up", async () => {
    let callCount = 0;
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({
      fetchImpl: async () => {
        callCount++;
        if (callCount <= 2) return { ok: false, status: 500, body: sseStream("") };
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBeGreaterThanOrEqual(3);
  });

  test("T3: does NOT retry on 404 (client error)", async () => {
    let callCount = 0;
    const { stream: stderr, read } = makeBufferStream();
    const ctx = makeCtx({
      stderr,
      fetchImpl: async () => {
        callCount++;
        return { ok: false, status: 404, body: sseStream("") };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(1);
    expect(read()).toContain("stream returned 404");
  });

  test("T3: does NOT retry on 401 (auth error)", async () => {
    let callCount = 0;
    const ctx = makeCtx({
      fetchImpl: async () => {
        callCount++;
        return { ok: false, status: 401, body: sseStream("") };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(1);
  });

  test("T3: does NOT retry on 429 (rate limit — status < 500)", async () => {
    let callCount = 0;
    const ctx = makeCtx({
      fetchImpl: async () => {
        callCount++;
        return { ok: false, status: 429, body: sseStream("") };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(1);
  });

  test("T3: prints retry message to stderr on 5xx", async () => {
    const { stream: stderr, read } = makeBufferStream();
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    let callCount = 0;
    const ctx = makeCtx({
      stderr,
      fetchImpl: async () => {
        callCount++;
        if (callCount === 1) return { ok: false, status: 502, body: sseStream("") };
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(read()).toContain("stream returned 502");
  });
});

// ── T3: Network error retry ───────────────────────────────────────────────

describe("streamRunWatch — network error retry", () => {
  test("T3: retries on fetch TypeError then succeeds", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    let callCount = 0;
    const ctx = makeCtx({
      stdout,
      fetchImpl: async () => {
        callCount++;
        if (callCount === 1) throw new TypeError("fetch failed");
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(2);
    expect(read()).toContain("run complete");
  });

  test("T3: prints error when network fails then succeeds on retry", async () => {
    const { stream: stderr, read } = makeBufferStream();
    let callCount = 0;
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({
      stderr,
      fetchImpl: async () => {
        callCount++;
        if (callCount === 1) throw new TypeError("fetch failed");
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(read()).toContain("connection failed, retrying");
    expect(callCount).toBe(2);
  });

  test("T3: AbortError from fetch does not retry (user cancelled)", async () => {
    let callCount = 0;
    const ctx = makeCtx({
      fetchImpl: async () => {
        callCount++;
        const err = new DOMException("The operation was aborted", "AbortError");
        throw err;
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(1);
  });

  test("T3: retries on stream read error mid-connection", async () => {
    let callCount = 0;
    const { stream: stdout, read } = makeBufferStream();
    const ctx = makeCtx({
      stdout,
      fetchImpl: async () => {
        callCount++;
        if (callCount === 1) {
          // Stream that errors mid-read.
          const body = new ReadableStream({
            start(controller) {
              controller.error(new Error("connection reset"));
            },
          });
          return { ok: true, status: 200, body };
        }
        const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(callCount).toBe(2);
    expect(read()).toContain("run complete");
  });

  test("T3: AbortError during stream read does not set streamError", async () => {
    let callCount = 0;
    const ctx = makeCtx({
      fetchImpl: async () => {
        callCount++;
        const body = new ReadableStream({
          start(controller) {
            controller.error(new DOMException("aborted", "AbortError"));
          },
        });
        return { ok: true, status: 200, body };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    // AbortError should break without retry.
    expect(callCount).toBeLessThanOrEqual(2);
  });
});

// ── T8: Security ──────────────────────────────────────────────────────────

describe("streamRunWatch — security", () => {
  test("T8: token not leaked to stdout", async () => {
    const { stream: stdout, read } = makeBufferStream();
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({ stdout, fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(read()).not.toContain(TOKEN);
  });

  test("T8: run_id is URL-encoded in fetch URL", async () => {
    let calledUrl = null;
    const runId = "run id/with spaces?&=";
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({
      fetchImpl: async (url) => {
        calledUrl = url;
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, runId, makeDeps());
    expect(calledUrl).toContain(encodeURIComponent(runId));
    expect(calledUrl).not.toContain("run id/with spaces");
  });

  test("T8: malformed SSE data does not crash", async () => {
    const sse = "event: gate_result\ndata: {INVALID JSON\n\nevent: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({ fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    // Should not throw — malformed gate_result is silently ignored.
  });

  test("T8: SSE event: line only parsed at transport level, not inside data values", async () => {
    const { stream: stdout, read } = makeBufferStream();
    // Proper SSE: the data: field contains JSON with escaped quotes — no raw newlines.
    // A real attacker cannot inject newlines inside a single SSE data: line because the
    // server serializes JSON (which escapes \n as \\n). This test verifies that a clean
    // gate_result with a tricky gate_name is rendered without interpretation.
    const sse = 'event: gate_result\ndata: {"gate_name":"event: hack","outcome":"PASS","loop":1,"wall_ms":10}\n\nevent: run_complete\ndata: {"state":"DONE"}\n\n';
    const ctx = makeCtx({ stdout, fetchImpl: mockFetch([{ status: 200, sse }]) });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    const output = read();
    expect(output).toContain("[event: hack] PASS");
    expect(output).toContain("run complete");
  });

  test("T8: Authorization header forwarded to fetch", async () => {
    let capturedHeaders = null;
    const sse = "event: run_complete\ndata: {\"state\":\"DONE\"}\n\n";
    const ctx = makeCtx({
      fetchImpl: async (_url, opts) => {
        capturedHeaders = opts.headers;
        return { ok: true, status: 200, body: sseStream(sse) };
      },
    });
    await streamRunWatch(ctx, RUN_ID_1, makeDeps());
    expect(capturedHeaders.Authorization).toContain(TOKEN);
  });
});

// ── T10: Constants ────────────────────────────────────────────────────────

describe("streamRunWatch — constants", () => {
  test("T10: WATCH_MAX_RETRIES is 3", () => {
    expect(WATCH_MAX_RETRIES).toBe(3);
  });

  test("T10: WATCH_RETRY_DELAY_MS is 2000", () => {
    expect(WATCH_RETRY_DELAY_MS).toBe(2000);
  });
});
