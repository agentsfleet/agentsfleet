/**
 * Revoke one `afc_…` CLI credential against the live API, surviving the
 * answers that mean "not yet" without ever mistaking them for "not yours".
 *
 * The acceptance lanes mint a credential per state dir and revoke it from a
 * single `afterAll`, so one transient refusal used to fail a whole spec file.
 * `UZ-AUTH-004` rides 503 and means the credential directory could not be
 * asked; the daemon's contract for that answer is "retry".
 */

export const CLI_CREDENTIALS_PATH = "/v1/cli-credentials";
const HTTP_METHOD_DELETE = "DELETE";
const HTTP_UNAUTHORIZED = 401;
const HTTP_NOT_FOUND = 404;
const HTTP_REQUEST_TIMEOUT = 408;
const HTTP_TOO_EARLY = 425;
const HTTP_TOO_MANY_REQUESTS = 429;
const HTTP_INTERNAL_SERVER_ERROR = 500;
const HTTP_BAD_GATEWAY = 502;
const HTTP_SERVICE_UNAVAILABLE = 503;
const HTTP_GATEWAY_TIMEOUT = 504;
/**
 * Answers that say the API could not decide yet, never that the credential is
 * bad. 500 is here because this DELETE is idempotent and the daemon answers
 * `UZ-INTERNAL-002` (500) for an I/O fault inside the revoke statement but
 * `UZ-AUTH-004` (503) for the same fault one step earlier, at acquire.
 */
const RETRYABLE_STATUSES: ReadonlySet<number> = new Set([
  HTTP_REQUEST_TIMEOUT,
  HTTP_TOO_EARLY,
  HTTP_TOO_MANY_REQUESTS,
  HTTP_INTERNAL_SERVER_ERROR,
  HTTP_BAD_GATEWAY,
  HTTP_SERVICE_UNAVAILABLE,
  HTTP_GATEWAY_TIMEOUT,
]);
export const MAX_REVOKE_ATTEMPTS = 5;
export const REVOKE_ATTEMPT_TIMEOUT_MS = 10_000;
const REVOKE_BACKOFF_BASE_MS = 500;
/**
 * ±20% on the schedule, so two lane processes retrying the same outage do not
 * re-hit the pool in lockstep; +0..20% on a `Retry-After`, which is a floor.
 */
const REVOKE_JITTER_RATIO = 0.2;
/** The daemon's admission shedding answers with `Retry-After`; it is honoured up to this. */
const REVOKE_RETRY_AFTER_CAP_MS = 10_000;
/** The longest one revoke can take: every attempt at its timeout, every wait at the cap plus jitter. */
export const REVOKE_WORST_CASE_MS = MAX_REVOKE_ATTEMPTS * REVOKE_ATTEMPT_TIMEOUT_MS +
  (MAX_REVOKE_ATTEMPTS - 1) * Math.round(REVOKE_RETRY_AFTER_CAP_MS * (1 + REVOKE_JITTER_RATIO));
const MS_PER_SECOND = 1000;
const RETRY_AFTER_HEADER = "Retry-After";
const TIMEOUT_ERROR_NAME = "TimeoutError";
const REVOKE_FAILED_NAME = "CliCredentialRevokeFailed";
export const REVOKE_LABEL = "CLI credential revoke";
const ERROR_DETAIL_MAX_CHARS = 200;

export interface MintedCliCredential {
  readonly id: string;
  readonly credential: string;
}

export interface RevokeOptions {
  /** Injectable so a test can assert the backoff schedule without waiting it out. */
  readonly sleep?: (ms: number) => Promise<void>;
  /** Injectable so a test can pin the jitter; 0.5 yields the un-jittered schedule. */
  readonly random?: () => number;
}

interface RevokeFault {
  readonly retryable: boolean;
  readonly retryAfterMs?: number | null;
  readonly cause?: unknown;
}

/**
 * A revoke the API did not complete. `retryable` is the server's word for an
 * answered request, and the transport's for one that never got an answer.
 */
export class CliCredentialRevokeFailed extends Error {
  readonly retryable: boolean;
  readonly retryAfterMs: number | null;

  constructor(message: string, fault: RevokeFault) {
    super(message, { cause: fault.cause });
    this.name = REVOKE_FAILED_NAME;
    this.retryable = fault.retryable;
    this.retryAfterMs = fault.retryAfterMs ?? null;
  }
}

/** A timed-out socket or a connection that failed says nothing about the row; anything else is ours. */
function transportFaultIsRetryable(cause: unknown): boolean {
  if (cause instanceof DOMException) return cause.name === TIMEOUT_ERROR_NAME;
  return cause instanceof TypeError;
}

function retryAfterMs(response: Response): number | null {
  const header = response.headers.get(RETRY_AFTER_HEADER);
  if (header === null) return null;
  const seconds = Number.parseInt(header, 10);
  if (!Number.isFinite(seconds) || seconds <= 0) return null;
  return Math.min(seconds * MS_PER_SECOND, REVOKE_RETRY_AFTER_CAP_MS);
}

function revokeBackoffMs(attempt: number, retryAfter: number | null, random: () => number): number {
  if (retryAfter !== null) {
    return Math.round(retryAfter + retryAfter * REVOKE_JITTER_RATIO * random());
  }
  const base = REVOKE_BACKOFF_BASE_MS * 2 ** (attempt - 1);
  return Math.round(base + base * REVOKE_JITTER_RATIO * (random() * 2 - 1));
}

/** One DELETE. Resolves `null` once the row is gone; otherwise the fault, never thrown. */
async function revokeOnce(
  apiUrl: string,
  minted: MintedCliCredential,
): Promise<CliCredentialRevokeFailed | null> {
  const label = `${REVOKE_LABEL} (credential ${minted.id})`;
  let response: Response;
  try {
    response = await fetch(`${apiUrl}${CLI_CREDENTIALS_PATH}/${encodeURIComponent(minted.id)}`, {
      method: HTTP_METHOD_DELETE,
      headers: { Authorization: `Bearer ${minted.credential}` },
      signal: AbortSignal.timeout(REVOKE_ATTEMPT_TIMEOUT_MS),
    });
  } catch (cause: unknown) {
    return new CliCredentialRevokeFailed(`${label} never reached the API: ${String(cause)}`, {
      retryable: transportFaultIsRetryable(cause),
      cause,
    });
  }
  // The row may already be gone: a test may have run `logout`, or an earlier
  // attempt revoked it and lost the answer, after which this bearer is refused
  // as revoked (`UZ-AUTH-023`, 401) or the row is not found (`UZ-AUTH-024`, 404).
  if (
    response.ok ||
    response.status === HTTP_UNAUTHORIZED ||
    response.status === HTTP_NOT_FOUND
  ) {
    return null;
  }
  const detail = await response.text().catch(() => "");
  return new CliCredentialRevokeFailed(
    `${label} answered ${response.status}: ${detail.slice(0, ERROR_DETAIL_MAX_CHARS)}`,
    { retryable: RETRYABLE_STATUSES.has(response.status), retryAfterMs: retryAfterMs(response) },
  );
}

/**
 * Revokes `minted`, retrying the answers that mean "not yet" with doubling,
 * jittered backoff. Gives up loudly: the thrown error names the credential
 * id (never the secret), the attempt count, and carries every attempt's
 * fault as its cause.
 */
export async function revokeCliCredential(
  apiUrl: string,
  minted: MintedCliCredential,
  options: RevokeOptions = {},
): Promise<void> {
  const sleep = options.sleep ?? ((ms: number) => Bun.sleep(ms));
  const random = options.random ?? Math.random;
  const faults: CliCredentialRevokeFailed[] = [];
  for (let attempt = 1; ; attempt += 1) {
    const fault = await revokeOnce(apiUrl, minted);
    if (fault === null) return;
    faults.push(fault);
    if (!fault.retryable || attempt >= MAX_REVOKE_ATTEMPTS) {
      if (faults.length === 1) throw fault;
      throw new CliCredentialRevokeFailed(`${fault.message} (gave up after ${attempt} attempts)`, {
        retryable: fault.retryable,
        cause: new AggregateError(faults),
      });
    }
    await sleep(revokeBackoffMs(attempt, fault.retryAfterMs, random));
  }
}
