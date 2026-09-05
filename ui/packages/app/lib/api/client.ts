import { ApiError, RequestCancelledError } from "./errors";
import { recordWorkspaceFetchForAcceptance } from "../acceptance/workspace-fetch-audit";
import { RETRY_CODE_TIMEOUT, runWithRetry, type RetryOptions } from "./retry";

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

// Per-attempt ceiling for a request whose caller passes no `signal`. The same
// window the SSE backfill grants its proxy fetch: long enough for a slow page
// read, short enough that a hung backend fails the render instead of pinning
// it. A caller with a stricter or looser budget passes its own signal and this
// default does not apply.
const DEFAULT_REQUEST_TIMEOUT_MS = 10_000;
// A client-side timeout has no server status. 408 is the closest standard
// meaning (the request did not complete in time) and is already in the retry
// layer's transient set, so a timed-out read is retried like any other blip.
const REQUEST_TIMEOUT_STATUS = 408;
const METHOD_GET = "GET";
const METHOD_HEAD = "HEAD";
const METHOD_PUT = "PUT";
// The methods `request()` retries on its own. Narrower than the policy's
// idempotency gate on purpose: a DELETE is idempotent in effect but not in
// answer — a 204 lost to the network comes back as a 404 on the replay, and the
// transport would then report a deletion that happened as a failure. Callers
// that want DELETE, POST or PATCH replayed say so through `requestWithRetry`.
const DEFAULT_RETRY_METHODS: ReadonlySet<string> = new Set([METHOD_GET, METHOD_HEAD, METHOD_PUT]);

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
// replaying it is the caller's decision, not the transport's.
export async function requestWithEtag<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<{ data: T; etag: string | null }> {
  const method = methodOf(init);
  recordAudit(path, method);
  const attempt = () => attemptWithEtag<T>(path, init, token);
  return DEFAULT_RETRY_METHODS.has(method)
    ? runWithRetry(attempt, method, { signal: init.signal ?? undefined })
    : attempt();
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
  return runWithRetry(async () => (await attemptWithEtag<T>(path, init, token)).data, method, {
    signal: init.signal ?? undefined,
    ...options,
  });
}

function methodOf(init: RequestInit): string {
  return (init.method ?? METHOD_GET).toUpperCase();
}

// Audited once per logical request, never per attempt: the acceptance budget
// counts what a render asked for, and a transient retry is not a second ask.
function recordAudit(path: string, method: string): void {
  if (method === METHOD_GET) recordWorkspaceFetchForAcceptance(path);
}

// One attempt: the fetch, the abort classification, and the RFC 7807 parse.
async function attemptWithEtag<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<{ data: T; etag: string | null }> {
  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, {
      ...init,
      // The caller's signal wins; only a request with none gets the default.
      signal: init.signal ?? AbortSignal.timeout(DEFAULT_REQUEST_TIMEOUT_MS),
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
    // A navigation abort is not a failure. Rethrowing the raw DOMException
    // leaves every caller to recognise it, and the ones that do not turn a
    // page the user already left into an unhandled rejection.
    if (isAbort(cause)) throw new RequestCancelledError(path);
    // A timeout IS a failure, and a transient one: the retry layer classifies
    // this code as retryable, so a hung read gets its second chance.
    if (isTimeout(cause)) {
      throw new ApiError(`request to ${path} timed out`, REQUEST_TIMEOUT_STATUS, RETRY_CODE_TIMEOUT);
    }
    throw cause;
  }

  const etag = etagFrom(res);

  if (res.status === 204) return { data: undefined as T, etag };

  // Error bodies are RFC 7807 problem+json: `{ docs_uri, title, detail,
  // error_code, request_id, user_message?, etag? }` (see
  // rustd/crates/afd_http/src/envelope.rs, ProblemResponse). `user_message`
  // (when present) is the curated dashboard-safe sentence for this code —
  // preferred over `detail`/`title`, which are written for the CLI/API
  // audience and often carry internal nouns a dashboard user can't act on.
  const body = await res.json().catch(() => ({ detail: res.statusText }));

  if (!res.ok) {
    const retryAfterMs = retryAfterFrom(res);
    throw new ApiError(
      body.user_message ?? body.detail ?? body.title ?? res.statusText,
      res.status,
      body.error_code ?? "UZ-UNKNOWN",
      body.request_id,
      retryAfterMs,
      // A 412 carries the resource's current etag in the body so the editor can
      // rebase without a second GET (REST guide §4).
      body.etag ?? etag,
    );
  }

  return { data: body as T, etag };
}
