import { ApiError, HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT, RequestCancelledError } from "./errors";
import { recordWorkspaceFetchForAcceptance } from "../acceptance/workspace-fetch-audit";
import { HTTP_METHOD, runWithRetry, type RetryOptions } from "./retry";
import { classifyFailure } from "./retry-classify";

// Full backend origin — used for display URLs (webhooks) and server-side fetches.
// No fallback on purpose: a silent api-dev default once pointed env-less
// worktrees at the shared dev backend. Production code refuses to guess;
// test lanes declare their target (vitest.setup.ts, playwright configs).
export function requireApiOrigin(): string {
  const value = process.env.NEXT_PUBLIC_API_URL;
  if (!value) {
    throw new Error(
      "NEXT_PUBLIC_API_URL is unset — refusing to guess a backend. " +
        "Run provision-env-1password; the repo's post-checkout hook links " +
        "ui/packages/app/.env.local from ~/.config/agentsfleet/ui.env.local.",
    );
  }
  return value;
}
export const API_ORIGIN = requireApiOrigin();

// BASE for fetch calls. On the server we hit the backend directly (no CORS).
// In the browser we go through the same-origin `/backend` proxy configured in
// next.config.ts `rewrites` — browser never sees a cross-origin request.
export const BASE = typeof window === "undefined" ? API_ORIGIN : "/backend";

// Per-attempt ceiling for a request whose caller passes no `signal`: long
// enough for a slow page read, short enough that a hung backend fails the
// render instead of pinning it. It matches the window the SSE backfill grants
// its proxy fetch (`lib/streaming/fleet-stream-backfill.ts`); the two are held
// equal by the pin in client.defaults.test.ts rather than by one importing the
// other, so the transport never depends on the streaming module. A caller with
// a stricter or looser budget passes its own signal and this default does not
// apply.
export const DEFAULT_REQUEST_TIMEOUT_MS = 10_000;

/**
 * The default per-attempt timeout, bounded by what remains of the policy's
 * deadline when the attempt starts: an attempt begun late in the run cannot
 * outlive the deadline by its own ceiling, so the deadline is the total it
 * says it is.
 */
export function attemptTimeoutMs(remainingMs: number): number {
  return Math.min(DEFAULT_REQUEST_TIMEOUT_MS, remainingMs);
}
// The methods `request()` retries on its own. Narrower than the policy's
// idempotency gate on purpose: a DELETE is idempotent in effect but not in
// answer — a 204 lost to the network comes back as a 404 on the replay, and the
// transport would then report a deletion that happened as a failure. Callers
// that want DELETE, POST or PATCH replayed say so through `requestWithRetry`.
const DEFAULT_RETRY_METHODS: ReadonlySet<string> = new Set([
  HTTP_METHOD.GET,
  HTTP_METHOD.HEAD,
  HTTP_METHOD.PUT,
]);

/**
 * Parses a `Retry-After` header value into milliseconds. Honors the
 * delta-seconds form (e.g., `Retry-After: 30`); the HTTP-date form is
 * rare for our APIs and is ignored (callers fall back to exponential
 * backoff). Mirrors the CLI parser at `cli/src/lib/http.js`.
 */
const MS_PER_SECOND = 1000;

// W3C Trace Context field widths, in hex characters. The server parses exactly
// this shape (`observability/trace.zig`); anything else is ignored and it
// starts a fresh root, so a malformed value here silently costs correlation
// rather than breaking the request.
const TRACE_ID_HEX_LEN = 32;
const SPAN_ID_HEX_LEN = 16;
const TRACEPARENT_VERSION = "00";
const TRACEPARENT_SAMPLED = "01";
const HEADER_TRACEPARENT = "traceparent";

const HEX_ALPHABET = "0123456789abcdef";

function randomHex(length: number): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  let out = "";
  for (const byte of bytes) out += HEX_ALPHABET[byte % HEX_ALPHABET.length];
  return out;
}

/**
 * A fresh W3C `traceparent` for one request, so a slow page can be attributed
 * to the server-side stages that produced it.
 *
 * Always a new ROOT rather than a continuation: the browser holds no span this
 * request is a child of, and inventing a parent id would attach the server's
 * span to something that never existed.
 */
export function newTraceparent(): string {
  return [
    TRACEPARENT_VERSION,
    randomHex(TRACE_ID_HEX_LEN),
    randomHex(SPAN_ID_HEX_LEN),
    TRACEPARENT_SAMPLED,
  ].join("-");
}

/**
 * True for the abort a caller asked for. `fetch` rejects with a DOMException
 * whose `name` is `AbortError`; some runtimes and test doubles surface a plain
 * Error instead, so the name is what is checked rather than the class.
 */
function isAbort(cause: unknown): boolean {
  return cause instanceof Error && cause.name === "AbortError";
}

/** True for the abort `AbortSignal.timeout` raises — `TimeoutError`, never `AbortError`. */
function isTimeout(cause: unknown): boolean {
  return cause instanceof Error && cause.name === "TimeoutError";
}

/** A cancel or a timeout: the two ways a request ends without an answer. */
export function isTransportInterrupt(cause: unknown): boolean {
  return isAbort(cause) || isTimeout(cause);
}

/**
 * Maps a fetch or body-read rejection to the transport's error classes. A
 * navigation abort is not a failure: rethrowing the raw DOMException leaves
 * every caller to recognise it, and the ones that do not turn a page the user
 * already left into an unhandled rejection. A timeout IS a failure, and a
 * transient one: the retry layer classifies its code as retryable, so a hung
 * read gets its second chance (a hung write does not — the policy's replay
 * gate refuses a POST or PATCH the server may already have processed).
 * Anything else is returned as it came, for the caller to throw.
 */
export function classifyTransportFailure(cause: unknown, path: string): unknown {
  if (isAbort(cause)) return new RequestCancelledError(path);
  if (isTimeout(cause)) {
    return new ApiError(`request to ${path} timed out`, HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT);
  }
  return cause;
}

// The RFC 7807 problem+json shape every error body carries — see
// rustd/crates/afd_http/src/envelope.rs, ProblemResponse.
type ProblemBody = {
  detail?: string;
  title?: string;
  error_code?: string;
  request_id?: string;
  user_message?: string;
  etag?: string;
};

// The signal that bounds the fetch bounds the body stream too, so a cancel or
// a timeout can land here as easily as before the headers arrived — and must
// mean the same thing, never a success body typed as `T`. A body that is not
// JSON (an intermediary's HTML 502 page) still needs a status to report, so
// that case keeps the status text as its detail. A stream that broke before
// the body ended is neither: it is the socket failure it was, for the policy
// to classify with the headers already in hand.
async function readBody(res: Response, path: string): Promise<unknown> {
  try {
    return await res.json();
  } catch (cause) {
    if (isTransportInterrupt(cause)) throw classifyTransportFailure(cause, path);
    if (cause instanceof SyntaxError) return { detail: res.statusText };
    throw cause;
  }
}

export function parseRetryAfterHeaderValue(headerVal: string | null): number | null {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n) && n >= 0) return n * MS_PER_SECOND;
  return null;
}

// Reads Retry-After off a response. Typed to need only an optional `Headers`
// so it tolerates header-less duck-typed responses (test doubles, exotic
// runtimes); a missing Headers reads as "no Retry-After" and the retry layer
// falls back to exponential backoff rather than throwing.
function retryAfterFrom(res: { headers?: Headers }): number | null {
  return res.headers ? parseRetryAfterHeaderValue(res.headers.get("retry-after")) : null;
}

// Reads the ETag off a response. Typed with optional `headers` for the same
// reason as `retryAfterFrom` — header-less duck-typed responses (test doubles)
// read as "no ETag" rather than throwing.
function etagFrom(res: { headers?: Headers }): string | null {
  return res.headers ? res.headers.get("etag") : null;
}

export async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  return (await requestWithEtag<T>(path, init, token)).data;
}

// Like `request`, but also surfaces the `ETag` response header. Used by the
// optimistic-concurrency surfaces (the fleet console's source editor, the
// catalog row editor): the caller holds the tag and sends it back as `If-Match`
// on the next write, so a concurrent edit is a 412 rather than a silent
// overwrite. `etag` is null when the endpoint sets no header.
//
// Reads (and the one replay-safe write, PUT) ride the retry policy by default.
// Every other write keeps one attempt unless its caller opts in through
// `requestWithRetry`: a timed-out POST may well have been processed, and
// replaying it is the caller's decision, not the transport's. The single
// attempt still runs under the policy so a cancel lands the same way.
export async function requestWithEtag<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<{ data: T; etag: string | null }> {
  const method = methodOf(init);
  recordAudit(path, method);
  const { data, etag } = await runWithRetry(classifiedAttempt<T>(path, init, token), method, {
    ...(DEFAULT_RETRY_METHODS.has(method) ? {} : { maxAttempts: 1 }),
    signal: init.signal ?? undefined,
    cancelled: cancelledFor(path),
    statusOf: statusOfAttempt,
  });
  return { data, etag };
}

// What the policy throws when the caller's signal aborts between attempts:
// the same cancel class a mid-flight abort produces, so a page the operator
// left is dropped silently whichever moment the navigation landed in.
function cancelledFor(path: string): () => Error {
  return () => new RequestCancelledError(path);
}

/**
 * `request` with an explicit retry configuration, for the callers that own
 * their replay decision — the steer POST, the thread and events reads. The
 * policy is `retry.ts`'s; its idempotency gate still refuses to replay a
 * non-idempotent method on a server 5xx.
 */
export async function requestWithRetry<T>(
  path: string,
  init: RequestInit,
  token: string,
  options: RetryOptions = {},
): Promise<T> {
  const method = methodOf(init);
  recordAudit(path, method);
  // One signal for the fetch and the schedule: a caller that cancels through
  // the policy's options cuts the request in flight, not only the next one.
  const signal = init.signal ?? options.signal;
  const { data } = await runWithRetry(classifiedAttempt<T>(path, { ...init, signal }, token), method, {
    cancelled: cancelledFor(path),
    statusOf: statusOfAttempt,
    ...options,
    signal,
  });
  return data;
}

function methodOf(init: RequestInit): string {
  return (init.method ?? HTTP_METHOD.GET).toUpperCase();
}

// Audited once per logical request, never per attempt: the acceptance budget
// counts what a render asked for, and a transient retry is not a second ask.
function recordAudit(path: string, method: string): void {
  if (method === HTTP_METHOD.GET) recordWorkspaceFetchForAcceptance(path);
}

type Attempt<T> = { data: T; etag: string | null; status: number };

function statusOfAttempt<T>(attempt: Attempt<T>): number {
  return attempt.status;
}

// One attempt as the policy sees it: the send and the read each classify
// their own failure with what only this side knows — whether the response
// headers had arrived — and the policy reads the provenance off the result.
function classifiedAttempt<T>(path: string, init: RequestInit, token: string): (remainingMs: number) => Promise<Attempt<T>> {
  return async (remainingMs: number) => {
    const res = await sendRequest(path, init, token, remainingMs).catch((cause: unknown) => {
      throw classifyFailure(cause, false);
    });
    return readResponse<T>(res, path).catch((cause: unknown) => {
      throw classifyFailure(cause, true);
    });
  };
}

// The send: the fetch and the abort classification.
async function sendRequest(path: string, init: RequestInit, token: string, remainingMs: number): Promise<Response> {
  try {
    return await fetch(`${BASE}${path}`, {
      ...init,
      // The caller's signal wins; only a request with none gets the default,
      // and the default is no longer than the deadline's remainder.
      signal: init.signal ?? AbortSignal.timeout(attemptTimeoutMs(remainingMs)),
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
        // Placed BEFORE the caller spread so an explicit traceparent wins —
        // a caller continuing an existing trace knows better than this default.
        [HEADER_TRACEPARENT]: newTraceparent(),
        ...init.headers,
      },
    });
  } catch (cause) {
    throw classifyTransportFailure(cause, path);
  }
}

// The read: the status, the ETag, and the RFC 7807 parse.
async function readResponse<T>(res: Response, path: string): Promise<Attempt<T>> {
  const etag = etagFrom(res);

  if (res.status === 204) return { data: undefined as T, etag, status: res.status };

  const body = await readBody(res, path);

  if (!res.ok) {
    // `user_message` (when present) is the curated dashboard-safe sentence for
    // this code — preferred over `detail`/`title`, which are written for the
    // CLI/API audience and often carry internal nouns a dashboard user can't
    // act on.
    const problem = body as ProblemBody;
    const retryAfterMs = retryAfterFrom(res);
    throw new ApiError(
      problem.user_message ?? problem.detail ?? problem.title ?? res.statusText,
      res.status,
      problem.error_code ?? "UZ-UNKNOWN",
      problem.request_id,
      retryAfterMs,
      // A 412 carries the resource's current etag in the body so the editor can
      // rebase without a second GET (REST guide §4).
      problem.etag ?? etag,
    );
  }

  return { data: body as T, etag, status: res.status };
}
