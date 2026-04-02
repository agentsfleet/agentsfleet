const DEFAULT_TIMEOUT_MS = 15000;

export class ApiError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = "ApiError";
    this.status = details.status;
    this.code = details.code;
    this.requestId = details.requestId;
    this.body = details.body;
  }
}

export async function apiRequest(url, options = {}) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl = options.fetchImpl || globalThis.fetch;
  if (typeof fetchImpl !== "function") {
    throw new ApiError("fetch is unavailable", { code: "NO_FETCH" });
  }

  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);

  try {
    const res = await fetchImpl(url, {
      method: options.method || "GET",
      headers: options.headers || {},
      body: options.body,
      signal: ctrl.signal,
    });

    const text = await res.text();
    let json = null;
    if (text.length > 0) {
      try {
        json = JSON.parse(text);
      } catch {
        json = null;
      }
    }

    if (!res.ok) {
      const errorCode = json?.error?.code || `HTTP_${res.status}`;
      const requestId = json?.error?.request_id ?? json?.request_id ?? null;
      const message = json?.error?.message || res.statusText || "request failed";
      throw new ApiError(message, {
        status: res.status,
        code: errorCode,
        requestId,
        body: json ?? text,
      });
    }

    return json ?? {};
  } catch (err) {
    if (err.name === "AbortError") {
      throw new ApiError(`request timed out after ${timeoutMs}ms`, {
        status: 408,
        code: "TIMEOUT",
      });
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * POST with SSE streaming response. Calls onEvent for each parsed SSE event.
 * Returns when the stream ends or an error occurs.
 */
export async function streamFetch(url, payload, headers, onEvent, options = {}) {
  const timeoutMs = options.timeoutMs ?? 30000;
  const fetchImpl = options.fetchImpl || globalThis.fetch;
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
      let json = null;
      try { json = JSON.parse(text); } catch { /* ignore */ }
      const errorCode = json?.error?.code || `HTTP_${res.status}`;
      const message = json?.error?.message || res.statusText || "request failed";
      throw new ApiError(message, { status: res.status, code: errorCode, body: json ?? text });
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
        if (event) onEvent(event);
      }
    }
  } catch (err) {
    if (err.name === "AbortError") {
      throw new ApiError(`stream timed out after ${timeoutMs}ms`, { status: 408, code: "TIMEOUT" });
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

function parseSseFrame(frame) {
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

export function authHeaders(auth) {
    const headers = {
        "Content-Type": "application/json",
    };

    if (auth?.token) {
        headers.Authorization = `Bearer ${auth.token}`;
        return headers;
    }

    if (auth?.apiKey) {
        headers.Authorization = `Bearer ${auth.apiKey}`;
    }

    return headers;
}
