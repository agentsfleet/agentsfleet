import * as Data from "effect/Data";
import { ApiError, HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT } from "./errors";

/**
 * What a failed attempt was and where it happened. The policy in `retry.ts`
 * reads two facts off every failure: its kind (which transient class it is,
 * or that it is none) and its provenance (whether the request provably never
 * left, whether the server may still hold it, or whether the server answered
 * without processing it). Provenance is what decides whether a write may be
 * sent again. Socket codes are read from the error's `cause`, where Node's
 * `fetch` puts them — never from the error's message.
 */

const HTTP_STATUS_TOO_EARLY = 425;
const HTTP_STATUS_TOO_MANY_REQUESTS = 429;
const HTTP_STATUS_BAD_GATEWAY = 502;
const HTTP_STATUS_SERVICE_UNAVAILABLE = 503;
const HTTP_STATUS_GATEWAY_TIMEOUT = 504;

export const FAILURE_KIND = {
  /** A 408 from the server, or the transport's own clock running out. */
  TIMEOUT: "timeout",
  /** 425 Too Early. */
  EARLY: "early",
  /** 429 Too Many Requests, usually with a `Retry-After`. */
  RATE: "rate",
  /** 502, 503 or 504: a gateway answering for a backend that did not. */
  SERVER: "server",
  /** No answer at all: a socket or resolver failure. */
  NETWORK: "network",
  /** Every other answer or error. Never retried. */
  FATAL: "fatal",
} as const;
export type FailureKind = (typeof FAILURE_KIND)[keyof typeof FAILURE_KIND];

export const PROVENANCE = {
  /** The request provably never left this process; any method may replay. */
  PRE_SEND: "pre-send",
  /** The server may hold the request; only an idempotent method may replay. */
  POST_SEND: "post-send",
  /** The server answered and declined to process it; any method may replay. */
  ANSWERED: "answered",
} as const;
export type Provenance = (typeof PROVENANCE)[keyof typeof PROVENANCE];

/** Every status the policy retries, with the kind it is reported as. */
const TRANSIENT_STATUS_KIND: ReadonlyMap<number, FailureKind> = new Map([
  [HTTP_STATUS_REQUEST_TIMEOUT, FAILURE_KIND.TIMEOUT],
  [HTTP_STATUS_TOO_EARLY, FAILURE_KIND.EARLY],
  [HTTP_STATUS_TOO_MANY_REQUESTS, FAILURE_KIND.RATE],
  [HTTP_STATUS_BAD_GATEWAY, FAILURE_KIND.SERVER],
  [HTTP_STATUS_SERVICE_UNAVAILABLE, FAILURE_KIND.SERVER],
  [HTTP_STATUS_GATEWAY_TIMEOUT, FAILURE_KIND.SERVER],
]);
export const TRANSIENT_STATUSES: readonly number[] = Array.from(TRANSIENT_STATUS_KIND.keys());

/**
 * Socket and resolver codes that prove the request never left: the connection
 * was refused, the name did not resolve, or the connect phase itself timed out
 * (`UND_ERR_CONNECT_TIMEOUT` is undici's, the fetch Node ships). Every other
 * code — a reset, a broken pipe, a headers or body timeout — arrives after
 * the request was on the wire, and so does a `TypeError` with no cause at
 * all, which is what a browser-shaped fetch throws.
 */
export const PRE_SEND_CODES: ReadonlySet<string> = new Set([
  "ECONNREFUSED",
  "ENOTFOUND",
  "EAI_AGAIN",
  "UND_ERR_CONNECT_TIMEOUT",
  "ConnectionRefused",
]);

/**
 * A failed attempt, classified. `cause` is the error the attempt threw, and it
 * is what the caller receives when the policy gives up: the classification is
 * the policy's business, never the caller's.
 */
export class ClassifiedFailure extends Data.TaggedError("ClassifiedFailure")<{
  readonly kind: FailureKind;
  readonly provenance: Provenance;
  readonly status: number | undefined;
  readonly retryAfterMs: number | null;
  readonly cause: unknown;
}> {}

function causeCode(err: Error): string | undefined {
  const { cause } = err;
  if (typeof cause !== "object" || cause === null || !("code" in cause)) return undefined;
  return typeof cause.code === "string" ? cause.code : undefined;
}

function classifyAnswer(err: ApiError): ClassifiedFailure {
  const facts = { status: err.status, retryAfterMs: err.retryAfterMs, cause: err };
  if (err.code === RETRY_CODE_TIMEOUT) {
    // The transport's clock ran out with the request on the wire.
    return new ClassifiedFailure({ ...facts, kind: FAILURE_KIND.TIMEOUT, provenance: PROVENANCE.POST_SEND });
  }
  const kind = TRANSIENT_STATUS_KIND.get(err.status);
  if (kind === undefined) {
    return new ClassifiedFailure({ ...facts, kind: FAILURE_KIND.FATAL, provenance: PROVENANCE.ANSWERED });
  }
  // A gateway's 5xx may follow a handler that ran; a 408, 425 or 429 is the
  // server saying it did not run one.
  const provenance = kind === FAILURE_KIND.SERVER ? PROVENANCE.POST_SEND : PROVENANCE.ANSWERED;
  return new ClassifiedFailure({ ...facts, kind, provenance });
}

function classifyNetwork(err: TypeError, sent: boolean): ClassifiedFailure {
  const code = causeCode(err);
  const neverLeft = !sent && code !== undefined && PRE_SEND_CODES.has(code);
  return new ClassifiedFailure({
    kind: FAILURE_KIND.NETWORK,
    provenance: neverLeft ? PROVENANCE.PRE_SEND : PROVENANCE.POST_SEND,
    status: undefined,
    retryAfterMs: null,
    cause: err,
  });
}

/**
 * Classifies whatever an attempt threw. `sent` is what the transport knows
 * that the error may not: once response headers arrived, a failure is
 * post-send whatever its code says. A failure already classified passes
 * through, so the transport may classify with what it knows and the policy
 * may classify what a bare thunk throws.
 */
export function classifyFailure(err: unknown, sent: boolean): ClassifiedFailure {
  if (err instanceof ClassifiedFailure) return err;
  if (err instanceof ApiError) return classifyAnswer(err);
  if (err instanceof TypeError) return classifyNetwork(err, sent);
  return new ClassifiedFailure({
    kind: FAILURE_KIND.FATAL,
    provenance: PROVENANCE.POST_SEND,
    status: undefined,
    retryAfterMs: null,
    cause: err,
  });
}
