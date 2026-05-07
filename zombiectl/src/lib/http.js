const DEFAULT_TIMEOUT_MS = 15000;
const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;
const RETRYABLE_STATUSES = new Set([408, 425, 429, 502, 503, 504]);

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

function classifyRetryable(err) {
  if (err instanceof ApiError) {
    if (err.code === "TIMEOUT") return "timeout";
    if (err.status && RETRYABLE_STATUSES.has(err.status)) {
      // Server can opt out of retries by sending Retry-After: 0; we
      // surface that on the body so the wrapper can honor it.
      if (err.body?.error?.retry_after_seconds === 0) return null;
      if (err.status === 429) return "429";
      return "5xx";
    }
    if (typeof err.code === "string" && /^UZ-[A-Z0-9]+-RETRY/.test(err.code)) {
      return "server_marked_retryable";
    }
    return null;
  }
  if (err instanceof TypeError && typeof err.message === "string" && err.message.toLowerCase().includes("fetch failed")) {
    return "network";
  }
  if (err && (err.code === "ECONNRESET" || err.code === "ETIMEDOUT" || err.code === "ENOTFOUND")) {
    return "network";
  }
  return null;
}

function backoffDelay({ attempt, baseDelayMs, capDelayMs, retryAfterMs, randomFn }) {
  if (typeof retryAfterMs === "number" && retryAfterMs > 0) {
    // Server-provided floor. Apply +0..20% jitter so a herd of clients
    // doesn't synchronize their next attempt.
    return retryAfterMs + retryAfterMs * 0.2 * randomFn();
  }
  const base = Math.min(baseDelayMs * Math.pow(2, attempt - 1), capDelayMs);
  // ±20% jitter centered on the base.
  const jitter = base * 0.2 * (randomFn() * 2 - 1);
  return Math.max(0, base + jitter);
}

function parseRetryAfterHeaderMs(headerVal) {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n)) return n * 1000;
  // HTTP-date form is rare for our APIs; fall back to ignoring.
  return null;
}

function noRetryEnv(env) {
  const v = env?.ZOMBIE_NO_RETRY;
  return v === "1" || v === "true";
}

function defaultSleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function apiRequestWithRetry(url, options = {}) {
  const retryCfg = options.retry ?? {};
  const maxAttemptsRaw = retryCfg.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  const baseDelayMs = retryCfg.baseDelayMs ?? DEFAULT_BASE_DELAY_MS;
  const capDelayMs = retryCfg.capDelayMs ?? DEFAULT_CAP_DELAY_MS;
  // Bounds enforcement per spec Invariant #4: maxAttempts ∈ [1, 10].
  // Out-of-range is misconfiguration, not a runtime decision.
  if (!Number.isInteger(maxAttemptsRaw) || maxAttemptsRaw < 1 || maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP) {
    throw new ApiError(`retry.maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`, {
      code: "CONFIG_INVALID",
    });
  }
  const env = options.env ?? (typeof process !== "undefined" ? process.env : {});
  const maxAttempts = noRetryEnv(env) ? 1 : maxAttemptsRaw;
  const sleep = options.sleepImpl ?? defaultSleep;
  const randomFn = options.randomFn ?? Math.random;
  const onAttempt = options.onAttempt;
  const onRetry = options.onRetry;

  let attempt = 0;
  let lastErr = null;
  while (attempt < maxAttempts) {
    attempt += 1;
    const startedAt = Date.now();
    try {
      const result = await apiRequest(url, options);
      const durationMs = Date.now() - startedAt;
      if (typeof onAttempt === "function") {
        onAttempt({ attempt, status: 200, durationMs, retryCount: attempt - 1, terminal: true });
      }
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const reason = classifyRetryable(err);
      const status = err instanceof ApiError ? err.status : undefined;
      const willRetry = reason !== null && attempt < maxAttempts;
      if (willRetry) {
        if (typeof onRetry === "function") {
          onRetry({ attempt, status, durationMs, reason });
        }
        const retryAfterMs = err instanceof ApiError ? parseRetryAfterHeaderMs(err.body?.headers?.["retry-after"]) : null;
        const delay = backoffDelay({ attempt, baseDelayMs, capDelayMs, retryAfterMs, randomFn });
        await sleep(delay);
        lastErr = err;
        continue;
      }
      if (typeof onAttempt === "function") {
        onAttempt({ attempt, status, durationMs, retryCount: attempt - 1, terminal: true });
      }
      throw err;
    }
  }
  // Defensive: while-loop should always either return or throw above.
  throw lastErr ?? new ApiError("apiRequestWithRetry exhausted without throw", { code: "INTERNAL" });
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
