import { describe, test, expect } from "bun:test";
import { streamFetch, ApiError } from "../src/lib/http.js";

// ── Test helpers ─────────────────────────────────────────────────────────────

function sseResponseFrom(sseBody, status = 200) {
  const encoder = new TextEncoder();
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "Error",
    headers: new Headers(),
    text: async () => sseBody,
    body: {
      getReader() {
        let sent = false;
        return {
          read() {
            if (!sent) { sent = true; return Promise.resolve({ done: false, value: encoder.encode(sseBody) }); }
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
    const events = [];
    const fetchImpl = async () => sseResponseFrom(
      'event: tool_use\ndata: {"id":"tu_01","name":"read_file"}\n\nevent: done\ndata: {"ok":true}\n\n'
    );
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(2);
    expect(events[0].type).toBe("tool_use");
    expect(events[0].data.name).toBe("read_file");
    expect(events[1].type).toBe("done");
  });

  test("merges custom headers with content-type and accept", async () => {
    let capturedHeaders = null;
    const fetchImpl = async (url, opts) => {
      capturedHeaders = opts.headers;
      return sseResponseFrom('event: done\ndata: {}\n\n');
    };
    await streamFetch("https://api.test.com/v1/x", { msg: 1 },
      { Authorization: "Bearer tok" }, () => {}, { fetchImpl });
    expect(capturedHeaders.Authorization).toBe("Bearer tok");
    expect(capturedHeaders["Content-Type"]).toBe("application/json");
    expect(capturedHeaders.Accept).toBe("text/event-stream");
  });

  test("sends JSON-stringified payload as body", async () => {
    let capturedBody = null;
    const fetchImpl = async (url, opts) => {
      capturedBody = opts.body;
      return sseResponseFrom('event: done\ndata: {}\n\n');
    };
    await streamFetch("https://api.test.com/v1/x", { messages: ["hello"], tools: [] },
      {}, () => {}, { fetchImpl });
    const parsed = JSON.parse(capturedBody);
    expect(parsed.messages).toEqual(["hello"]);
  });
});

// ── T2: Edge cases ──────────────────────────────────────────────────────────

describe("streamFetch — edge cases", () => {
  test("handles empty SSE body (no events)", async () => {
    const events = [];
    const fetchImpl = async () => sseResponseFrom("");
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(0);
  });

  test("skips heartbeat comments", async () => {
    const events = [];
    const fetchImpl = async () => sseResponseFrom(': heartbeat\n\nevent: done\ndata: {"ok":true}\n\n');
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("done");
  });

  test("handles multi-chunk delivery (split across reads)", async () => {
    const encoder = new TextEncoder();
    const chunk1 = 'event: text_delta\ndata: {"te';
    const chunk2 = 'xt":"hello"}\n\nevent: done\ndata: {}\n\n';
    let readCount = 0;
    const fetchImpl = async () => ({
      ok: true, status: 200, body: {
        getReader() {
          return {
            read() {
              readCount++;
              if (readCount === 1) return Promise.resolve({ done: false, value: encoder.encode(chunk1) });
              if (readCount === 2) return Promise.resolve({ done: false, value: encoder.encode(chunk2) });
              return Promise.resolve({ done: true });
            },
          };
        },
      },
    });
    const events = [];
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events).toHaveLength(2);
    expect(events[0].data.text).toBe("hello");
  });
});

// ── T3: Error paths ─────────────────────────────────────────────────────────

describe("streamFetch — error paths", () => {
  test("non-200 response throws ApiError with parsed error code", async () => {
    const fetchImpl = async () => ({
      ok: false,
      status: 403,
      statusText: "Forbidden",
      text: async () => JSON.stringify({ error: { code: "UZ-AUTH-001", message: "access denied" } }),
    });
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect(err.status).toBe(403);
      expect(err.code).toBe("UZ-AUTH-001");
    }
  });

  test("non-200 with non-JSON body still throws ApiError", async () => {
    const fetchImpl = async () => ({
      ok: false,
      status: 502,
      statusText: "Bad Gateway",
      text: async () => "upstream error",
    });
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect(err.code).toBe("HTTP_502");
    }
  });

  test("timeout throws ApiError with TIMEOUT code", async () => {
    const fetchImpl = async (url, opts) => {
      // Simulate slow response — wait longer than timeoutMs
      await new Promise((resolve, reject) => {
        opts.signal.addEventListener("abort", () => {
          reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
        });
      });
    };
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, {
        fetchImpl,
        timeoutMs: 50,
      });
      expect(true).toBe(false);
    } catch (err) {
      expect(err).toBeInstanceOf(ApiError);
      expect(err.code).toBe("TIMEOUT");
    }
  });

  test("read error mid-stream propagates", async () => {
    const fetchImpl = async () => ({
      ok: true,
      status: 200,
      body: {
        getReader() {
          return {
            read() { return Promise.reject(new Error("socket hang up")); },
          };
        },
      },
    });
    try {
      await streamFetch("https://api.test.com/v1/x", {}, {}, () => {}, { fetchImpl });
      expect(true).toBe(false);
    } catch (err) {
      expect(err.message).toContain("socket hang up");
    }
  });
});

// ── T4: Output fidelity — SSE parsing correctness ───────────────────────────

describe("streamFetch — SSE parsing fidelity", () => {
  test("parses event with JSON data containing nested objects", async () => {
    const events = [];
    const fetchImpl = async () => sseResponseFrom(
      'event: tool_use\ndata: {"id":"tu_01","name":"read_file","input":{"path":"src/main.go"}}\n\n'
    );
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events[0].data.input.path).toBe("src/main.go");
  });

  test("handles data with unicode content", async () => {
    const events = [];
    const fetchImpl = async () => sseResponseFrom(
      'event: text_delta\ndata: {"text":"中文テスト 👨‍💻"}\n\n'
    );
    await streamFetch("https://api.test.com/v1/x", {}, {}, (e) => events.push(e), { fetchImpl });
    expect(events[0].data.text).toBe("中文テスト 👨‍💻");
  });
});
