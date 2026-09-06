// HTTP retry-with-backoff layer over the core `apiRequest` transport.
// Owns retry classification, exponential backoff + jitter, Retry-After
// honoring, the AGENTSFLEET_NO_RETRY escape hatch, and the replay gate:
// a non-idempotent method is sent again only when the failure provably
// happened before the request left, or when the server answered without
// running it. Split out of http.ts so transport and retry concerns stay
// separable and each module stays under the line cap. The dashboard's
// policy (ui/packages/app/lib/api/retry.ts) makes the same decisions;
// samples/fixtures/retry-policy/cases.json is the table both are proven
// against.

import { ApiError, apiRequest, type ApiRequestOptions } from "./http.ts";

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;
// The longest wait a server or intermediary can ask for and still be obeyed;
// above it the answer is surfaced at once. The dashboard's policy caps at the same value.
const DEFAULT_RETRY_AFTER_CAP_MS = 10_000;
const RETRYABLE_STATUSES = new Set<number>([408, 425, 429, 502, 503, 504]);
const RETRY_REASON_429 = "429" as const;
const RETRY_REASON_5XX = "5xx" as const;
const HTTP_METHOD_GET = "GET" as const;
const RETRY_REASON_NETWORK = "network" as const;
const TYPE_OBJECT = "object" as const;
const STATUS_TIMEOUT = "timeout" as const;
const HTTP_STATUS_SERVER_ERROR_FLOOR = 500;
// The request provably never left this process.
const PROVENANCE_PRE_SEND = "pre-send" as const;
// The server may hold the request: a reset after sending, a timeout, a 5xx.
const PROVENANCE_POST_SEND = "post-send" as const;
// The server answered and declined to run the request.
const PROVENANCE_ANSWERED = "answered" as const;
// Socket and resolver codes that prove the request never left. Node's fetch
// puts them on the error's `cause` (`UND_ERR_CONNECT_TIMEOUT` is undici's);
// Bun's puts them on the error itself (`ConnectionRefused` is Bun's).
export const PRE_SEND_CODES: ReadonlySet<string> = new Set([
  "ECONNREFUSED",
  "ENOTFOUND",
  "EAI_AGAIN",
  "UND_ERR_CONNECT_TIMEOUT",
  "ConnectionRefused",
]);

// Reasons surfaced on the `onRetry` callback so the analytics layer
// can attribute the retry to a concrete failure class.
export type RetryReason =
  | typeof STATUS_TIMEOUT
  | typeof RETRY_REASON_429
  | typeof RETRY_REASON_5XX
  | typeof RETRY_REASON_NETWORK;

type Provenance =
  | typeof PROVENANCE_PRE_SEND
  | typeof PROVENANCE_POST_SEND
  | typeof PROVENANCE_ANSWERED;

interface Classified {
  readonly reason: RetryReason;
  readonly provenance: Provenance;
}

function hasRetryOptOut(body: unknown): boolean {
  if (body === null || typeof body !== TYPE_OBJECT) return false;
  const errField = (body as { error?: unknown }).error;
  if (errField === null || typeof errField !== TYPE_OBJECT) return false;
  return (errField as { retry_after_seconds?: unknown }).retry_after_seconds === 0;
}

function codeOf(value: unknown): string | undefined {
  if (!(value instanceof Object) || !("code" in value)) return undefined;
  return typeof value.code === "string" ? value.code : undefined;
}

// The code is on the error under Bun and on its cause under Node.
function socketCode(err: Error): string | undefined {
  return codeOf(err) ?? codeOf(err.cause);
}

function classifyRetryable(err: unknown): Classified | null {
  if (err instanceof ApiError) {
    // The transport's own clock ran out with the request on the wire.
    if (err.code === "TIMEOUT") return { reason: STATUS_TIMEOUT, provenance: PROVENANCE_POST_SEND };
    if (err.status !== undefined && RETRYABLE_STATUSES.has(err.status)) {
      // Server can opt out of retries by sending Retry-After: 0; we
      // surface that on the body so the wrapper can honor it.
      if (hasRetryOptOut(err.body)) return null;
      if (err.status === 429) return { reason: RETRY_REASON_429, provenance: PROVENANCE_ANSWERED };
      // A 408 or 425 is the server declining to run the request; a 5xx is a
      // gateway answering for a handler that may have run.
      const provenance = err.status >= HTTP_STATUS_SERVER_ERROR_FLOOR ? PROVENANCE_POST_SEND : PROVENANCE_ANSWERED;
      return { reason: RETRY_REASON_5XX, provenance };
    }
    return null;
  }
  // A network failure is a TypeError carrying the socket code, or none at
  // all — which is read as sent, since nothing proves otherwise.
  if (err instanceof TypeError) {
    const code = socketCode(err);
    const provenance = code !== undefined && PRE_SEND_CODES.has(code) ? PROVENANCE_PRE_SEND : PROVENANCE_POST_SEND;
    return { reason: RETRY_REASON_NETWORK, provenance };
  }
  return null;
}

interface BackoffArgs {
  attempt: number;
  baseDelayMs: number;
  capDelayMs: number;
  retryAfterMs: number | null;
  randomFn: () => number;
}

function backoffDelay({ attempt, baseDelayMs, capDelayMs, retryAfterMs, randomFn }: BackoffArgs): number {
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

function noRetryEnv(env: NodeJS.ProcessEnv | undefined): boolean {
  const v = env?.AGENTSFLEET_NO_RETRY;
  return v === "1" || v === "true";
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export interface RetryConfig {
  maxAttempts?: number;
  baseDelayMs?: number;
  capDelayMs?: number;
  retryAfterCapMs?: number;
}

export interface AttemptInfo {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  retryCount: number;
  terminal: boolean;
}

export interface RetryInfo {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  reason: RetryReason;
}

export interface ApiRequestWithRetryOptions extends ApiRequestOptions {
  // null/undefined → use apiRequestWithRetry's defaults.
  retry?: RetryConfig | null | undefined;
  env?: NodeJS.ProcessEnv;
  sleepImpl?: (ms: number) => Promise<void>;
  randomFn?: () => number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
}

/**
 * HTTP methods safe to replay. A genuine server 5xx (>=500) may have been
 * processed upstream before the gateway error surfaced, so replaying a
 * non-idempotent method (POST/PATCH) risks a duplicate mutation. Mirrors the
 * Supabase CLI's `isRetryableResponse` idempotency gate.
 */
export function isIdempotentMethod(method: string): boolean {
  const m = method.toUpperCase();
  return m === HTTP_METHOD_GET || m === "PUT" || m === "DELETE" || m === "HEAD";
}

interface ResolvedRetryRuntime {
  maxAttempts: number;
  baseDelayMs: number;
  capDelayMs: number;
  retryAfterCapMs: number;
  sleep: (ms: number) => Promise<void>;
  randomFn: () => number;
  onAttempt: ((info: AttemptInfo) => void) | undefined;
  onRetry: ((info: RetryInfo) => void) | undefined;
  method: string;
}

function resolveRetryRuntime(options: ApiRequestWithRetryOptions): ResolvedRetryRuntime {
  const retryCfg = options.retry ?? {};
  const maxAttemptsRaw = retryCfg.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  // Bounds enforcement per Invariant #4: maxAttempts ∈ [1, 10]. Out-of-range
  // is misconfiguration, not a runtime decision.
  if (!Number.isInteger(maxAttemptsRaw) || maxAttemptsRaw < 1 || maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP) {
    throw new ApiError(`retry.maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`, {
      code: "CONFIG_INVALID",
    });
  }
  const env = options.env ?? (typeof process !== "undefined" ? process.env : undefined);
  return {
    maxAttempts: noRetryEnv(env) ? 1 : maxAttemptsRaw,
    baseDelayMs: retryCfg.baseDelayMs ?? DEFAULT_BASE_DELAY_MS,
    capDelayMs: retryCfg.capDelayMs ?? DEFAULT_CAP_DELAY_MS,
    retryAfterCapMs: retryCfg.retryAfterCapMs ?? DEFAULT_RETRY_AFTER_CAP_MS,
    sleep: options.sleepImpl ?? defaultSleep,
    randomFn: options.randomFn ?? Math.random,
    onAttempt: options.onAttempt,
    onRetry: options.onRetry,
    method: options.method ?? HTTP_METHOD_GET,
  };
}

function emitTerminalAttempt(
  onAttempt: ((info: AttemptInfo) => void) | undefined,
  attempt: number,
  status: number | undefined,
  durationMs: number,
): void {
  if (onAttempt !== undefined) {
    onAttempt({ attempt, status, durationMs, retryCount: attempt - 1, terminal: true });
  }
}

interface AttemptContext {
  attempt: number;
  status: number | undefined;
  durationMs: number;
}

// Decides whether a failed attempt retries. Returns the backoff delay (and
// fires onRetry) when it should, else null. The replay gate blocks a
// non-idempotent method whenever the server may hold the request.
function planRetry(
  err: unknown,
  cfg: ResolvedRetryRuntime,
  ctx: AttemptContext,
): { delayMs: number } | null {
  const classified = classifyRetryable(err);
  if (classified === null) return null;
  const unsafeReplay = classified.provenance === PROVENANCE_POST_SEND && !isIdempotentMethod(cfg.method);
  const retryAfterMs = err instanceof ApiError ? err.retryAfterMs : null;
  const waitTooLong = retryAfterMs !== null && retryAfterMs > cfg.retryAfterCapMs;
  if (unsafeReplay || waitTooLong || ctx.attempt >= cfg.maxAttempts) return null;
  if (cfg.onRetry !== undefined) {
    cfg.onRetry({ attempt: ctx.attempt, status: ctx.status, durationMs: ctx.durationMs, reason: classified.reason });
  }
  const delayMs = backoffDelay({
    attempt: ctx.attempt,
    baseDelayMs: cfg.baseDelayMs,
    capDelayMs: cfg.capDelayMs,
    retryAfterMs,
    randomFn: cfg.randomFn,
  });
  return { delayMs };
}

export async function apiRequestWithRetry(
  url: string,
  options: ApiRequestWithRetryOptions = {},
): Promise<unknown> {
  const cfg = resolveRetryRuntime(options);
  async function runAttempt(attempt: number): Promise<unknown> {
    const startedAt = Date.now();
    try {
      const result = await apiRequest(url, options);
      emitTerminalAttempt(cfg.onAttempt, attempt, 200, Date.now() - startedAt);
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const status = err instanceof ApiError ? err.status : undefined;
      const step = planRetry(err, cfg, { attempt, status, durationMs });
      if (step) {
        await cfg.sleep(step.delayMs);
        return runAttempt(attempt + 1);
      }
      emitTerminalAttempt(cfg.onAttempt, attempt, status, durationMs);
      throw err;
    }
  }
  return runAttempt(1);
}
