/**
 * Revoke one `afc_…` CLI credential against the live API, surviving the
 * answers that mean "not yet" without ever mistaking them for "not yours".
 *
 * The acceptance lanes mint a credential per state dir and revoke it from a
 * single `afterAll`, so one transient refusal used to fail a whole spec file.
 * Which answers mean "not yet", and how long to wait between attempts, is
 * `retryable.ts`'s; this file decides only what a revoke's answers mean.
 */

import {
  ATTEMPT_TIMEOUT_MS,
  RETRYABLE_STATUSES,
  type RetryOptions,
  retryAfterMs,
  transportFaultIsRetryable,
  withRetries,
} from "./retryable.ts";

export const CLI_CREDENTIALS_PATH = "/v1/cli-credentials";
const HTTP_METHOD_DELETE = "DELETE";
const HTTP_UNAUTHORIZED = 401;
const HTTP_NOT_FOUND = 404;
const REVOKE_FAILED_NAME = "CliCredentialRevokeFailed";
export const REVOKE_LABEL = "CLI credential revoke";
const ERROR_DETAIL_MAX_CHARS = 200;

export interface MintedCliCredential {
  readonly id: string;
  readonly credential: string;
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

/** One DELETE. Answers `ok` once the row is gone; otherwise the fault, never thrown. */
async function revokeOnce(
  apiUrl: string,
  minted: MintedCliCredential,
): Promise<{ readonly ok: true } | { readonly fault: CliCredentialRevokeFailed }> {
  const label = `${REVOKE_LABEL} (credential ${minted.id})`;
  let response: Response;
  try {
    response = await fetch(`${apiUrl}${CLI_CREDENTIALS_PATH}/${encodeURIComponent(minted.id)}`, {
      method: HTTP_METHOD_DELETE,
      headers: { Authorization: `Bearer ${minted.credential}` },
      signal: AbortSignal.timeout(ATTEMPT_TIMEOUT_MS),
    });
  } catch (cause: unknown) {
    const fault = new CliCredentialRevokeFailed(`${label} never reached the API: ${String(cause)}`, {
      retryable: transportFaultIsRetryable(cause),
      cause,
    });
    return { fault };
  }
  // The row may already be gone: a test may have run `logout`, or an earlier
  // attempt revoked it and lost the answer, after which this bearer is refused
  // as revoked (`UZ-AUTH-023`, 401) or the row is not found (`UZ-AUTH-024`, 404).
  if (
    response.ok ||
    response.status === HTTP_UNAUTHORIZED ||
    response.status === HTTP_NOT_FOUND
  ) {
    return { ok: true };
  }
  const detail = await response.text().catch(() => "");
  const fault = new CliCredentialRevokeFailed(
    `${label} answered ${response.status}: ${detail.slice(0, ERROR_DETAIL_MAX_CHARS)}`,
    { retryable: RETRYABLE_STATUSES.has(response.status), retryAfterMs: retryAfterMs(response) },
  );
  return { fault };
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
  options: RetryOptions = {},
): Promise<void> {
  const retried = await withRetries(() => revokeOnce(apiUrl, minted), options);
  if ("ok" in retried) return;
  const { last, faults } = retried;
  if (faults.length === 1) throw last;
  throw new CliCredentialRevokeFailed(`${last.message} (gave up after ${faults.length} attempts)`, {
    retryable: last.retryable,
    cause: new AggregateError(faults),
  });
}
