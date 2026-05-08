import { test, expect } from "bun:test";
import { streamGet } from "../src/lib/sse.js";

// streamGet covers the full GET-stream lifecycle: header injection,
// frame boundary slicing, JSON-parsed event delivery, error mapping,
// timeout, abort, and graceful early termination via onEvent → false.

function fakeStream(chunks) {
  let i = 0;
  return {
    getReader() {
      return {
        async read() {
          if (i >= chunks.length) return { done: true };
          const value = new TextEncoder().encode(chunks[i++]);
          return { done: false, value };
        },
      };
    },
  };
}

function fakeFetch(impl) {
  return (url, init) => Promise.resolve(impl(url, init));
}

test("streamGet decodes \\n\\n-separated frames into parsed events", async () => {
  const fetchImpl = fakeFetch(() => ({
    ok: true,
    body: fakeStream([
      "id: 1\nevent: chunk\ndata: {\"text\":\"a\"}\n\n",
      "id: 2\nevent: done\ndata: {\"text\":\"b\"}\n\n",
    ]),
  }));
  const events = [];
  await streamGet("https://example/x", {}, (e) => { events.push(e); }, { fetchImpl, timeoutMs: 0 });
  expect(events).toHaveLength(2);
  expect(events[0]).toEqual({ id: "1", type: "chunk", data: { text: "a" } });
  expect(events[1]).toEqual({ id: "2", type: "done", data: { text: "b" } });
});

test("streamGet stops early when onEvent returns false", async () => {
  const fetchImpl = fakeFetch(() => ({
    ok: true,
    body: fakeStream([
      "event: a\ndata: 1\n\n",
      "event: b\ndata: 2\n\n",
    ]),
  }));
  const events = [];
  await streamGet("https://example/x", {}, (e) => {
    events.push(e);
    return false;
  }, { fetchImpl, timeoutMs: 0 });
  expect(events).toHaveLength(1);
});

test("streamGet throws ApiError when the response is non-2xx", async () => {
  const fetchImpl = fakeFetch(() => ({
    ok: false,
    status: 500,
    statusText: "Server Error",
    text: async () => JSON.stringify({ error: { code: "UZ-EXT-001", message: "boom" } }),
  }));
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 0 }),
  ).rejects.toMatchObject({ status: 500, code: "UZ-EXT-001" });
});

test("streamGet throws when fetch is unavailable", async () => {
  // Pass an explicit non-function impl AND temporarily knock out
  // globalThis.fetch so the typeof guard can fire.
  const originalFetch = globalThis.fetch;
  // @ts-expect-error - intentional knock-out for the NO_FETCH branch
  globalThis.fetch = undefined;
  try {
    await expect(
      streamGet("https://example/x", {}, () => {}, { timeoutMs: 0 }),
    ).rejects.toMatchObject({ code: "NO_FETCH" });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("streamGet throws when the response body is not streamable", async () => {
  const fetchImpl = fakeFetch(() => ({ ok: true, body: null }));
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 0 }),
  ).rejects.toMatchObject({ code: "NO_STREAM_BODY" });
});

test("streamGet returns silently when the external signal is already aborted", async () => {
  const ctrl = new AbortController();
  ctrl.abort();
  const fetchImpl = fakeFetch(() => {
    const err = new Error("aborted");
    err.name = "AbortError";
    throw err;
  });
  await streamGet(
    "https://example/x",
    {},
    () => {},
    { fetchImpl, timeoutMs: 0, signal: ctrl.signal },
  );
});

test("streamGet maps a timeout AbortError to ApiError(TIMEOUT, 408)", async () => {
  const fetchImpl = fakeFetch(() => {
    const err = new Error("timed out");
    err.name = "AbortError";
    throw err;
  });
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 1 }),
  ).rejects.toMatchObject({ status: 408, code: "TIMEOUT" });
});

test("streamGet sends Accept: text/event-stream and merges caller headers", async () => {
  let captured;
  const fetchImpl = fakeFetch((_, init) => {
    captured = init;
    return { ok: true, body: fakeStream([]) };
  });
  await streamGet("https://example/x", { Authorization: "Bearer x" }, () => {}, { fetchImpl, timeoutMs: 0 });
  expect(captured.method).toBe("GET");
  expect(captured.headers.Accept).toBe("text/event-stream");
  expect(captured.headers.Authorization).toBe("Bearer x");
});
