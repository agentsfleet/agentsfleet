import { apiRequestWithRetry, authHeaders } from "../lib/http.js";
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

export { apiHeaders, request };
