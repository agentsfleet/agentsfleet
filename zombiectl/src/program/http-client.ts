import {
  apiRequestWithRetry,
  authHeaders,
  type ApiRequestWithRetryOptions,
  type AttemptInfo,
  type FetchImpl,
  type RetryConfig,
  type RetryInfo,
} from "../lib/http.ts";
import { trackHttpRequest, trackHttpRetry } from "../lib/analytics.ts";

// request() always overrides fetchImpl / retry / onAttempt / onRetry from ctx,
// so callers cannot pass them. sleepImpl / randomFn / env flow through to
// apiRequestWithRetry — needed by tests to keep backoff deterministic.
export type RequestOptions = Omit<
  ApiRequestWithRetryOptions,
  "fetchImpl" | "retry" | "onAttempt" | "onRetry"
>;

// Subset of the runtime CLI context that the HTTP layer reads. Commands
// pass a richer object (workspace, env, streams, etc.); structural
// typing lets the extra fields ride along untyped here. Full ctx
// shape is defined at D40 alongside the program/* migration.
export interface HttpRequestContext {
  apiUrl: string;
  token?: string | null;
  apiKey?: string | null;
  fetchImpl?: FetchImpl;
  retryConfig?: RetryConfig | null;
  analyticsClient?: unknown;
  distinctId?: string;
}

export function apiHeaders(ctx: HttpRequestContext): Record<string, string> {
  return authHeaders({ token: ctx.token, apiKey: ctx.apiKey });
}

// ctx.retryConfig is the single carrier that lets runCommand({ retry })
// flow into the HTTP layer without changing apiRequest's signature.
// Falsy → use apiRequestWithRetry's default (3 attempts). Object →
// propagate verbatim (incl. { maxAttempts: 1 } from runCommand({ retry:
// false })). ZOMBIE_NO_RETRY=1 still wins inside apiRequestWithRetry as
// the global short-circuit.
export async function request(
  ctx: HttpRequestContext,
  reqPath: string,
  options: RequestOptions = {},
): Promise<unknown> {
  const url = `${ctx.apiUrl}${reqPath}`;
  const method = options.method ?? "GET";
  const analyticsClient = ctx.analyticsClient ?? null;
  const distinctId = ctx.distinctId ?? "anonymous";
  return apiRequestWithRetry(url, {
    ...options,
    fetchImpl: ctx.fetchImpl,
    retry: ctx.retryConfig ?? undefined,
    onAttempt: (info: AttemptInfo) => {
      trackHttpRequest(analyticsClient, distinctId, {
        url: reqPath,
        method,
        status: info.status,
        duration_ms: info.durationMs,
        attempt: info.attempt,
        retry_count: info.retryCount,
      });
    },
    onRetry: (info: RetryInfo) => {
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
