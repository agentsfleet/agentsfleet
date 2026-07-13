// POST-based SSE consumer. Used by the execute proxy + agent stream.
//
// Mirror module to lib/sse.ts (which owns the GET-based stream). The
// split is by HTTP verb because POST requires a request body and the
// onEvent semantics differ slightly (no early-return on `false` here;
// the caller drains every frame). ApiError + FetchImpl are imported
// from http.ts so the two transports share their error vocabulary.

import { ApiError, readProblemDetails, type FetchImpl } from "./http.ts";

const DEFAULT_TIMEOUT_MS = 30_000;

export interface SseEvent {
  type: string;
  data: unknown;
}

export interface StreamFetchOptions {
  timeoutMs?: number;
  fetchImpl?: FetchImpl | undefined;
}

export async function streamFetch(
  url: string,
  payload: unknown,
  headers: Record<string, string>,
  onEvent: (event: SseEvent) => void,
  options: StreamFetchOptions = {},
): Promise<void> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl: FetchImpl | undefined = options.fetchImpl ?? (globalThis.fetch as FetchImpl | undefined);
  if (typeof fetchImpl !== "function") {
    throw new ApiError("fetch is unavailable", { code: "NO_FETCH" });
  }
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);

  try {
    const res = await fetchImpl(url, {
      method: "POST",
      headers: { ...headers, "Content-Type": "application/json", "Accept": "text/event-stream" },
      body: JSON.stringify(payload),
      signal: ctrl.signal,
    });

    if (!res.ok) {
      const text = await res.text();
      let json: unknown = null;
      try { json = JSON.parse(text); } catch { /* ignore */ }
      const problem = readProblemDetails(json);
      const errorCode = problem.code ?? `HTTP_${res.status}`;
      const message = problem.message ?? res.statusText ?? "request failed";
      throw new ApiError(message, { status: res.status, code: errorCode, body: json ?? text });
    }

    if (res.body === null) {
      throw new ApiError("response body is not streamable", { code: "NO_STREAM_BODY" });
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });

      let boundary = buf.indexOf(LITERAL);
      while (boundary !== -1) {
        const frame = buf.slice(0, boundary);
        buf = buf.slice(boundary + 2);
        const event = parseSseFrame(frame);
        if (event) onEvent(event);
        boundary = buf.indexOf(LITERAL);
      }
    }
  } catch (err) {
    if (err !== null && typeof err === TYPE_OBJECT && (err as { name?: unknown }).name === "AbortError") {
      throw new ApiError(`stream timed out after ${timeoutMs}ms`, { status: 408, code: "TIMEOUT" });
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

// Minimal SSE frame parser for the POST transport. The GET transport
// (lib/sse.ts) carries `id` and treats `data:` continuations; this one
// only needs `event:` + `data:` because the POST stream is short-lived
// and never asks the server to resume from an id.
function parseSseFrame(frame: string): SseEvent | null {
  let type = "message";
  let data = "";
  for (const line of frame.split("\n")) {
    if (line.startsWith("event: ")) type = line.slice(7);
    else if (line.startsWith("data: ")) data = line.slice(6);
    else if (line.startsWith(":")) continue; // comment/heartbeat
  }
  if (!data) return null;
  try { return { type, data: JSON.parse(data) }; } catch { return { type, data }; }
}
const LITERAL = "\n\n" as const;
const TYPE_OBJECT = "object" as const;
