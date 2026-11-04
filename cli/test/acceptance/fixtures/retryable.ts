/**
 * The retry policy the acceptance fixtures share against the live API: which
 * answers mean "not yet", how long to wait between attempts, and the one loop
 * that runs them.
 *
 * Two sources answer "not yet" without saying anything about the request. The
 * daemon's admission shedding answers 429 and 503 with `Retry-After`, and
 * `UZ-AUTH-004` rides 503 when the credential directory could not be asked.
 * The Cloudflare edge answers 502 with an HTML page for a daemon still
 * starting after a deploy, which is what turned the Nov 03, 2026 deploy lane
 * red on one credential mint.
 */

const HTTP_REQUEST_TIMEOUT = 408;
const HTTP_TOO_EARLY = 425;
const HTTP_TOO_MANY_REQUESTS = 429;
const HTTP_INTERNAL_SERVER_ERROR = 500;
const HTTP_BAD_GATEWAY = 502;
const HTTP_SERVICE_UNAVAILABLE = 503;
const HTTP_GATEWAY_TIMEOUT = 504;

/**
 * Answers that say the API could not decide yet, never that the request is
 * bad. 500 is here because the revoke `DELETE` is idempotent and the daemon
 * answers `UZ-INTERNAL-002` (500) for an I/O fault inside the revoke
 * statement but `UZ-AUTH-004` (503) for the same fault one step earlier.
 */
export const RETRYABLE_STATUSES: ReadonlySet<number> = new Set([
  HTTP_REQUEST_TIMEOUT,
  HTTP_TOO_EARLY,
  HTTP_TOO_MANY_REQUESTS,
  HTTP_INTERNAL_SERVER_ERROR,
  HTTP_BAD_GATEWAY,
  HTTP_SERVICE_UNAVAILABLE,
  HTTP_GATEWAY_TIMEOUT,
]);
/**
 * The subset a call that is not idempotent may repeat: the daemon shed it
 * (429) or refused it before reading it (425, 503 at acquire). A 502 joins
 * them only when the edge answered it (see `edgeAnswered`); a 502 the daemon
 * answered, a 500, a 504 or a timeout may follow a write that landed, which
 * is the policy `cli/src/lib/http-retry.ts` holds for every POST.
 */
export const UNPROCESSED_STATUSES: ReadonlySet<number> = new Set([
  HTTP_TOO_EARLY,
  HTTP_TOO_MANY_REQUESTS,
  HTTP_SERVICE_UNAVAILABLE,
]);

/**
 * Whether a 502 came from the edge rather than the daemon: the daemon answers
 * every refusal with a JSON problem document, and the Cloudflare edge answers
 * an origin it cannot reach with an HTML page. The edge cannot have handed
 * the request to a daemon that was not listening. A reply lost after the
 * daemon wrote is still possible behind the edge; that residual risk is the
 * acceptance lane's to carry, on a fixture account, never production login's.
 */
export function edgeAnswered(status: number, detail: string): boolean {
  return status === HTTP_BAD_GATEWAY && !detail.trimStart().startsWith("{");
}
/** Attempts in all, the first included. */
export const MAX_ATTEMPTS = 5;
/** How long one attempt may wait for its answer. */
export const ATTEMPT_TIMEOUT_MS = 10_000;
const BACKOFF_BASE_MS = 500;
/**
 * ±20% on the schedule, so two lane processes retrying the same outage do not
 * re-hit the pool in lockstep; +0..20% on a `Retry-After`, which is a floor.
 */
const JITTER_RATIO = 0.2;
/** A `Retry-After` is honoured up to this. */
const RETRY_AFTER_CAP_MS = 10_000;
/** The longest one call can take: every attempt at its timeout, every wait at the cap plus jitter. */
export const WORST_CASE_MS = MAX_ATTEMPTS * ATTEMPT_TIMEOUT_MS +
  (MAX_ATTEMPTS - 1) * Math.round(RETRY_AFTER_CAP_MS * (1 + JITTER_RATIO));
const MS_PER_SECOND = 1000;
const RETRY_AFTER_HEADER = "Retry-After";
const TIMEOUT_ERROR_NAME = "TimeoutError";

export interface RetryOptions {
  /** Injectable so a test can assert the backoff schedule without waiting it out. */
  readonly sleep?: (ms: number) => Promise<void>;
  /** Injectable so a test can pin the jitter; 0.5 yields the un-jittered schedule. */
  readonly random?: () => number;
}

/** What one failed attempt tells the loop. */
export interface RetryFault {
  readonly retryable: boolean;
  readonly retryAfterMs: number | null;
}

/** One attempt's answer: the value, or the fault the loop decides on. */
export type Attempt<T, F extends RetryFault> = { readonly ok: T } | { readonly fault: F };

/** How the loop ended: the value, or the deciding fault and every fault in order. */
export type Retried<T, F extends RetryFault> =
  | { readonly ok: T }
  | { readonly last: F; readonly faults: readonly F[] };

export function retryAfterMs(response: Response): number | null {
  const header = response.headers.get(RETRY_AFTER_HEADER);
  if (header === null) return null;
  const seconds = Number.parseInt(header, 10);
  if (!Number.isFinite(seconds) || seconds <= 0) return null;
  return Math.min(seconds * MS_PER_SECOND, RETRY_AFTER_CAP_MS);
}

export function backoffMs(attempt: number, retryAfter: number | null, random: () => number): number {
  if (retryAfter !== null) {
    return Math.round(retryAfter + retryAfter * JITTER_RATIO * random());
  }
  const base = BACKOFF_BASE_MS * 2 ** (attempt - 1);
  return Math.round(base + base * JITTER_RATIO * (random() * 2 - 1));
}

/** A timed-out socket or a connection that failed says nothing about the request; anything else is ours. */
export function transportFaultIsRetryable(cause: unknown): boolean {
  if (cause instanceof DOMException) return cause.name === TIMEOUT_ERROR_NAME;
  return cause instanceof TypeError;
}

/**
 * Runs `once` until it answers a value, a fault that is not retryable, or the
 * attempt cap, sleeping the backoff between attempts. The caller decides what
 * to throw from the faults; this loop never does.
 */
export async function withRetries<T, F extends RetryFault>(
  once: () => Promise<Attempt<T, F>>,
  options: RetryOptions = {},
): Promise<Retried<T, F>> {
  const sleep = options.sleep ?? ((ms: number) => Bun.sleep(ms));
  const random = options.random ?? Math.random;
  const faults: F[] = [];
  for (let attempt = 1; ; attempt += 1) {
    const answered = await once();
    if ("ok" in answered) return answered;
    faults.push(answered.fault);
    if (!answered.fault.retryable || attempt >= MAX_ATTEMPTS) {
      return { last: answered.fault, faults };
    }
    await sleep(backoffMs(attempt, answered.fault.retryAfterMs, random));
  }
}
