// GET-based Server-Sent Events consumer.
//
// `lib/http.js::streamFetch` is POST-only (used by the execute proxy);
// the M42 events endpoint is GET. We consume frames via fetch +
// ReadableStream, which lets us set Authorization headers (the native
// EventSource API can not).
//
// Each parsed frame is `{ id, type, data }` where `data` has been
// JSON.parse()'d if possible. Lines starting with `:` are comments
// (heartbeats) and skipped.

import { ApiError } from "./http.js";

const DEFAULT_TIMEOUT_MS = 60_000;

export async function streamGet(url, headers, onEvent, options = {}) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl = options.fetchImpl || globalThis.fetch;
  if (typeof fetchImpl !== "function") {
    throw new ApiError("fetch is unavailable", { code: "NO_FETCH" });
  }

  const ctrl = new AbortController();
  const externalSignal = options.signal;
  if (externalSignal) {
    if (externalSignal.aborted) ctrl.abort();
    else externalSignal.addEventListener("abort", () => ctrl.abort(), { once: true });
  }
  const timer = timeoutMs > 0 ? setTimeout(() => ctrl.abort(), timeoutMs) : null;

  try {
    const res = await fetchImpl(url, {
      method: "GET",
      headers: { ...headers, Accept: "text/event-stream" },
      signal: ctrl.signal,
    });
    if (!res.ok) {
      const text = await res.text();
      let json = null;
      try { json = JSON.parse(text); } catch { /* ignore */ }
      const errorCode = json?.error?.code || json?.error_code || `HTTP_${res.status}`;
      const message = json?.error?.message || json?.detail || res.statusText || "stream request failed";
      throw new ApiError(message, { status: res.status, code: errorCode, body: json ?? text });
    }

    if (!res.body || typeof res.body.getReader !== "function") {
      throw new ApiError("response body is not streamable", { code: "NO_STREAM_BODY" });
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let boundary;
      while ((boundary = buf.indexOf("\n\n")) !== -1) {
        const frame = buf.slice(0, boundary);
        buf = buf.slice(boundary + 2);
        const event = parseSseFrame(frame);
        if (event) {
          const cont = onEvent(event);
          if (cont === false) return;
        }
      }
    }
  } catch (err) {
    if (err.name === "AbortError") {
      if (externalSignal?.aborted) return; // user-cancelled, not a timeout
      throw new ApiError(`stream timed out after ${timeoutMs}ms`, { status: 408, code: "TIMEOUT" });
    }
    throw err;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function parseSseFrame(frame) {
  let id = null;
  let type = "message";
  let data = "";
  for (const raw of frame.split("\n")) {
    const line = raw.replace(/\r$/, "");
    if (!line) continue;
    if (line.startsWith(":")) continue; // comment
    if (line.startsWith("id: ")) id = line.slice(4);
    else if (line.startsWith("id:")) id = line.slice(3).trimStart();
    else if (line.startsWith("event: ")) type = line.slice(7);
    else if (line.startsWith("event:")) type = line.slice(6).trimStart();
    else if (line.startsWith("data: ")) data = data.length > 0 ? `${data}\n${line.slice(6)}` : line.slice(6);
    else if (line.startsWith("data:")) {
      const v = line.slice(5).trimStart();
      data = data.length > 0 ? `${data}\n${v}` : v;
    }
  }
  if (!data) return null;
  let parsed = data;
  try { parsed = JSON.parse(data); } catch { /* keep raw */ }
  return { id, type, data: parsed };
}
