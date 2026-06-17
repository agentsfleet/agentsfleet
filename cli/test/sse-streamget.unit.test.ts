import { test, expect } from "bun:test";
import { streamGet, type SseFrame } from "../src/lib/sse.ts";
import { asFetchImpl, type ResponseLike } from "./helpers.ts";

// streamGet covers the full GET-stream lifecycle: header injection,
// frame boundary slicing, JSON-parsed event delivery, error mapping,
// timeout, abort, and graceful early termination via onEvent → false.

interface FakeReader {
  read(): Promise<{ done: true } | { done: false; value: Uint8Array }>;
}
interface FakeStream { getReader(): FakeReader }

function fakeStream(chunks: string[]): FakeStream {
  let i = 0;
  return {
    getReader() {
      return {
        async read() {
          if (i >= chunks.length) return { done: true } as const;
          const chunk = chunks[i++];
          const value = new TextEncoder().encode(chunk ?? "");
          return { done: false, value } as const;
        },
      };
    },
  };
}

type FakeFetchResult = ResponseLike & { body?: FakeStream | null };

function fakeFetch(impl: (url: string, init?: RequestInit) => FakeFetchResult | Promise<FakeFetchResult>) {
  return asFetchImpl((url, init) => Promise.resolve(impl(url, init)) as Promise<ResponseLike>);
}

// Partial fake — apiRequest paths called by streamGet only touch a
// subset of Response. ResponseLike's required fields are filled when
// the production code reads them; tests stay minimal.
function partialResponse(over: Partial<ResponseLike> & { body?: FakeStream | null }): FakeFetchResult {
  return {
    ok: true,
    status: 200,
    statusText: "OK",
    headers: { get: () => null },
    text: async () => "",
    ...over,
  };
}

test("streamGet decodes \\n\\n-separated frames into parsed events", async () => {
  const fetchImpl = fakeFetch(() => partialResponse({
    body: fakeStream([
      "id: 1\nevent: chunk\ndata: {\"text\":\"a\"}\n\n",
      "id: 2\nevent: done\ndata: {\"text\":\"b\"}\n\n",
    ]),
  }));
  const events: SseFrame[] = [];
  await streamGet("https://example/x", {}, (e) => { events.push(e); }, { fetchImpl, timeoutMs: 0 });
  expect(events).toHaveLength(2);
  expect(events[0]).toEqual({ id: "1", type: "chunk", data: { text: "a" } });
  expect(events[1]).toEqual({ id: "2", type: "done", data: { text: "b" } });
});

test("streamGet stops early when onEvent returns false", async () => {
  const fetchImpl = fakeFetch(() => partialResponse({
    body: fakeStream([
      "event: a\ndata: 1\n\n",
      "event: b\ndata: 2\n\n",
    ]),
  }));
  const events: SseFrame[] = [];
  await streamGet("https://example/x", {}, (e) => {
    events.push(e);
    return false;
  }, { fetchImpl, timeoutMs: 0 });
  expect(events).toHaveLength(1);
});

test("streamGet throws ApiError when the response is non-2xx", async () => {
  const fetchImpl = fakeFetch(() => partialResponse({
    ok: false,
    status: 500,
    statusText: "Server Error",
    text: async () => JSON.stringify({ error: { code: "UZ-EXT-001", message: "boom" } }),
  }));
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 0 }),
  ).rejects.toMatchObject({ status: 500, code: "UZ-EXT-001" });
});

test("streamGet non-JSON error bodies fall back to status text and raw body", async () => {
  const fetchImpl = fakeFetch(() => partialResponse({
    ok: false,
    status: 502,
    statusText: "Bad Gateway",
    text: async () => "upstream unavailable",
  }));
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 0 }),
  ).rejects.toMatchObject({
    status: 502,
    code: "HTTP_502",
    message: "Bad Gateway",
    body: "upstream unavailable",
  });
});

test("streamGet throws when fetch is unavailable", async () => {
  // Pass an explicit non-function impl AND temporarily knock out
  // globalThis.fetch so the typeof guard can fire.
  const originalFetch = globalThis.fetch;
  // exactOptionalPropertyTypes blocks `globalThis.fetch = undefined`;
  // delete erases the property, which the NO_FETCH typeof-guard sees as
  // `undefined` at runtime same as assignment did.
  delete (globalThis as { fetch?: typeof fetch }).fetch;
  try {
    await expect(
      streamGet("https://example/x", {}, () => {}, { timeoutMs: 0 }),
    ).rejects.toMatchObject({ code: "NO_FETCH" });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("streamGet throws when the response body is not streamable", async () => {
  const fetchImpl = fakeFetch(() => partialResponse({ body: null }));
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

test("streamGet returns silently when the external signal aborts in flight", async () => {
  const ctrl = new AbortController();
  let requestSignal: AbortSignal | undefined;
  const fetchImpl = fakeFetch((_url, init) => {
    requestSignal = init?.signal as AbortSignal | undefined;
    return new Promise<FakeFetchResult>((_resolve, reject) => {
      requestSignal?.addEventListener("abort", () => {
        const err = new Error("aborted");
        err.name = "AbortError";
        reject(err);
      }, { once: true });
      queueMicrotask(() => ctrl.abort());
    });
  });
  await streamGet(
    "https://example/x",
    {},
    () => {},
    { fetchImpl, timeoutMs: 0, signal: ctrl.signal },
  );
  expect(requestSignal?.aborted).toBe(true);
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

test("streamGet timeout aborts the in-flight request", async () => {
  let requestSignal: AbortSignal | undefined;
  const fetchImpl = fakeFetch((_url, init) => {
    requestSignal = init?.signal as AbortSignal | undefined;
    return new Promise<FakeFetchResult>((_resolve, reject) => {
      requestSignal?.addEventListener("abort", () => {
        const err = new Error("timed out");
        err.name = "AbortError";
        reject(err);
      }, { once: true });
    });
  });
  await expect(
    streamGet("https://example/x", {}, () => {}, { fetchImpl, timeoutMs: 1 }),
  ).rejects.toMatchObject({ status: 408, code: "TIMEOUT" });
  expect(requestSignal?.aborted).toBe(true);
});

test("streamGet sends Accept: text/event-stream and merges caller headers", async () => {
  let captured: RequestInit | undefined;
  const fetchImpl = fakeFetch((_url, init) => {
    captured = init;
    return partialResponse({ body: fakeStream([]) });
  });
  await streamGet("https://example/x", { Authorization: "Bearer x" }, () => {}, { fetchImpl, timeoutMs: 0 });
  expect(captured?.method).toBe("GET");
  const headers = captured?.headers as Record<string, string> | undefined;
  expect(headers?.Accept).toBe("text/event-stream");
  expect(headers?.Authorization).toBe("Bearer x");
});
