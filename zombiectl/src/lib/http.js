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

export function authHeaders(auth) {
  const headers = {
    "Content-Type": "application/json",
  };

  if (auth?.token) {
    headers.Authorization = `Bearer ${auth.token}`;
    return headers;
  }

  if (auth?.apiKey) {
    headers["X-API-Key"] = auth.apiKey;
  }

  return headers;
}
