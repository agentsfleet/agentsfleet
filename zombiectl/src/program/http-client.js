import {
  ApiError,
  apiRequest,
  apiRequestWithRetry,
  authHeaders,
} from "../lib/http.js";
import { trackHttpRequest, trackHttpRetry } from "../lib/analytics.js";

function apiHeaders(ctx) {
  return authHeaders({ token: ctx.token, apiKey: ctx.apiKey });
}

// Per M63_004 §3: ctx.retryConfig is the single carrier that lets
// runCommand({ retry }) flow into the HTTP layer without changing
// apiRequest's signature. Falsy → use apiRequestWithRetry's default
// (3 attempts). Object → propagate verbatim (incl. { maxAttempts: 1 }
// from runCommand({ retry: false })). ZOMBIE_NO_RETRY=1 still wins
// inside apiRequestWithRetry as the global short-circuit.
async function request(ctx, reqPath, options = {}) {
  const url = `${ctx.apiUrl}${reqPath}`;
  const method = options.method || "GET";
  const analyticsClient = ctx.analyticsClient ?? null;
  const distinctId = ctx.distinctId ?? "anonymous";
  return apiRequestWithRetry(url, {
    ...options,
    fetchImpl: ctx.fetchImpl,
    retry: ctx.retryConfig ?? undefined,
    onAttempt: (info) => {
      trackHttpRequest(analyticsClient, distinctId, {
        url: reqPath,
        method,
        status: info.status,
        duration_ms: info.durationMs,
        attempt: info.attempt,
        retry_count: info.retryCount,
      });
    },
    onRetry: (info) => {
      trackHttpRetry(analyticsClient, distinctId, {
        url: reqPath,
        method,
        status: info.status,
        attempt: info.attempt,
        reason: info.reason,
      });
    },
  });
}

// Internal escape hatch — callers that explicitly need a single-shot
// fetch (no retry, no instrumentation) can call apiRequest directly
// via this re-export. Used by tests; not for handler code.
const requestNoRetry = apiRequest;

function printApiError(stderr, err, jsonMode, printJson, writeLine) {
  if (!(err instanceof ApiError)) throw err;
  const payload = {
    error: {
      code: err.code || "API_ERROR",
      message: err.message,
      status: err.status || null,
      request_id: err.requestId || null,
    },
  };
  if (jsonMode) {
    printJson(stderr, payload);
  } else {
    writeLine(stderr, `error: ${payload.error.code} ${payload.error.message}`);
    if (payload.error.request_id) writeLine(stderr, `request_id: ${payload.error.request_id}`);
  }
}

export {
  ApiError,
  apiHeaders,
  printApiError,
  request,
  requestNoRetry,
};
