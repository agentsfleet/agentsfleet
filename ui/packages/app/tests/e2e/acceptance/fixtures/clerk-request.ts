import { parseRetryAfterHeaderValue } from "@/lib/api/client";

const CLERK_API_BASE = "https://api.clerk.com/v1";
const CLERK_SECRET_KEY_ENV = "CLERK_SECRET_KEY";
const AUTHORIZATION_HEADER = "Authorization";
const CONTENT_TYPE_HEADER = "Content-Type";
const JSON_CONTENT_TYPE = "application/json";
const RETRY_AFTER_HEADER = "retry-after";
const CLERK_REQUEST_MAX_ATTEMPTS = 4;
const CLERK_REQUEST_TIMEOUT_MS = 15_000;
const CLERK_RETRY_BASE_DELAY_MS = 500;
const CLERK_RETRY_MAX_DELAY_MS = 5_000;
const CLERK_RETRY_BACKOFF_FACTOR = 2;
const CLERK_RETRY_JITTER_RATIO = 0.2;
const CLERK_RATE_LIMIT_STATUS = 429;
const CLERK_RETRYABLE_SERVER_STATUSES = new Set([502, 503, 504]);
const IDEMPOTENT_METHODS = new Set(["GET", "PUT", "DELETE", "HEAD"]);
const UNKNOWN_NETWORK_ERROR = "unknown network error";
const NETWORK_RETRY_CAUSE = "network";

export async function clerkRequest<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await requestClerkResponse(method, path, body);
  if (!response.ok) {
    const detail = await response.text();
    throw new Error(`Clerk ${method} ${path} → ${response.status}: ${detail}`);
  }
  return (await response.json()) as T;
}

async function requestClerkResponse(
  method: string,
  path: string,
  body: unknown,
): Promise<Response> {
  let attempt = 1;
  while (true) {
    let response: Response;
    try {
      response = await fetchClerk(method, path, body);
    } catch (error) {
      if (!isIdempotent(method) || attempt >= CLERK_REQUEST_MAX_ATTEMPTS) {
        throw clerkNetworkError(method, attempt, error);
      }
      await waitForClerkRetry(errorName(error), attempt, null);
      attempt += 1;
      continue;
    }

    if (!isRetryableResponse(method, response) || attempt >= CLERK_REQUEST_MAX_ATTEMPTS) {
      return response;
    }

    const retryAfterMs = parseRetryAfterHeaderValue(
      response.headers.get(RETRY_AFTER_HEADER),
    );
    if (response.body) await response.body.cancel();
    await waitForClerkRetry(`http_${response.status}`, attempt, retryAfterMs);
    attempt += 1;
  }
}

function fetchClerk(method: string, path: string, body: unknown): Promise<Response> {
  return fetch(`${CLERK_API_BASE}${path}`, {
    method,
    headers: authHeaders(),
    body: body !== undefined ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(CLERK_REQUEST_TIMEOUT_MS),
  });
}

function authHeaders(): Record<string, string> {
  const secret = process.env[CLERK_SECRET_KEY_ENV];
  if (!secret) throw new Error(`${CLERK_SECRET_KEY_ENV} missing`);
  return {
    [AUTHORIZATION_HEADER]: `Bearer ${secret}`,
    [CONTENT_TYPE_HEADER]: JSON_CONTENT_TYPE,
  };
}

function isIdempotent(method: string): boolean {
  return IDEMPOTENT_METHODS.has(method.toUpperCase());
}

function isRetryableResponse(method: string, response: Response): boolean {
  if (response.status === CLERK_RATE_LIMIT_STATUS) return true;
  return isIdempotent(method) && CLERK_RETRYABLE_SERVER_STATUSES.has(response.status);
}

function clerkNetworkError(method: string, attempts: number, error: unknown): Error {
  const detail = error instanceof Error ? error.message : UNKNOWN_NETWORK_ERROR;
  const attemptLabel = attempts === 1 ? "attempt" : "attempts";
  return new Error(`Clerk ${method} request failed after ${attempts} ${attemptLabel}: ${detail}`, {
    cause: error,
  });
}

function errorName(error: unknown): string {
  return error instanceof Error ? error.name : NETWORK_RETRY_CAUSE;
}

async function waitForClerkRetry(
  cause: string,
  attempt: number,
  retryAfterMs: number | null,
): Promise<void> {
  const backoffMs =
    CLERK_RETRY_BASE_DELAY_MS * CLERK_RETRY_BACKOFF_FACTOR ** (attempt - 1);
  const delayFloorMs = retryAfterMs ?? backoffMs;
  const jitterMs = Math.floor(delayFloorMs * CLERK_RETRY_JITTER_RATIO * Math.random());
  const delayMs = Math.min(delayFloorMs + jitterMs, CLERK_RETRY_MAX_DELAY_MS);
  process.stderr.write(
    `[e2e:auth] Clerk retry cause=${cause} next_attempt=${attempt + 1}/${CLERK_REQUEST_MAX_ATTEMPTS} delay_ms=${delayMs}\n`,
  );
  await new Promise((resolve) => setTimeout(resolve, delayMs));
}
