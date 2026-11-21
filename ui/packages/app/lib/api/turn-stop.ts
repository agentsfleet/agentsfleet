// The composer's Stop for every thread: one PATCH of a turn's status, sent from
// the browser to the same-origin turn route of a fleet or a draft, which mints
// the bearer and calls the daemon. A `fetch`, not a Server Action, for the
// reason the steer is one: Next runs one tab's Server Actions one at a time, so
// a Stop would queue behind a send.
//
// Every outcome is a `TurnStopResult`, never a throw. A 409 is its own kind:
// the turn finished before the Stop landed, so its answer stands and the
// conversation already shows it.

import type { TurnDetail } from "./turns";

/** The one status a Stop writes (`PATCH …/turns/{turn_id}`). */
export const TURN_STOP_STATUS = "cancelled";

/** Bounded so a hung proxy never leaves Stop pressed forever. */
const STOP_TIMEOUT_MS = 30_000;
const HTTP_CONFLICT = 409;
const CONTENT_TYPE_JSON = "application/json";

export const TURN_STOP_KIND = {
  /** The daemon cancelled the turn; `answer` is what the door answered with. */
  STOPPED: "stopped",
  /** The turn was answered or failed first; its answer stands. */
  FINISHED: "finished",
  /** No answer, or a refusal: the turn's state is unknown here. */
  FAILED: "failed",
} as const;

export type TurnStopResult<T> =
  | { kind: typeof TURN_STOP_KIND.STOPPED; answer: T }
  | { kind: typeof TURN_STOP_KIND.FINISHED }
  | { kind: typeof TURN_STOP_KIND.FAILED; status?: number };

/**
 * Asks the daemon to cancel the turn at `url` and the run serving it. `read`
 * turns the 200 body into what the caller applies, or `undefined` when the
 * body is not that, so each door keeps its own answer shape.
 */
export async function stopTurn<T>(
  url: string,
  signal: AbortSignal,
  read: (body: unknown) => T | undefined,
): Promise<TurnStopResult<T>> {
  let res: Response;
  try {
    res = await fetch(url, {
      method: "PATCH",
      headers: { "Content-Type": CONTENT_TYPE_JSON },
      body: JSON.stringify({ status: TURN_STOP_STATUS }),
      signal: AbortSignal.any([signal, AbortSignal.timeout(STOP_TIMEOUT_MS)]),
      cache: "no-store",
      // A signed-out request is answered with a redirect to sign-in; followed,
      // the sign-in page would read as the turn.
      redirect: "manual",
    });
  } catch {
    return { kind: TURN_STOP_KIND.FAILED };
  }
  if (res.status === HTTP_CONFLICT) return { kind: TURN_STOP_KIND.FINISHED };
  if (res.type === "opaqueredirect" || !res.ok) {
    return { kind: TURN_STOP_KIND.FAILED, ...(res.status > 0 ? { status: res.status } : {}) };
  }
  const answer = read(await res.json().catch(() => null));
  return answer === undefined ? { kind: TURN_STOP_KIND.FAILED } : { kind: TURN_STOP_KIND.STOPPED, answer };
}

/** A fleet door's answer: the saved row, by the fields the stream settles it by. */
export function readTurnDetail(body: unknown): TurnDetail | undefined {
  return isTurn(body) ? body : undefined;
}

function isTurn(body: unknown): body is TurnDetail {
  return typeof body === "object" && body !== null
    && "turn_id" in body && typeof body.turn_id === "string"
    && "status" in body && typeof body.status === "string";
}
