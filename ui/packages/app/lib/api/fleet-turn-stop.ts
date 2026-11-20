// The composer's Stop: one PATCH of a turn's status, sent from the browser to
// the same-origin turn route, which mints the bearer and calls the daemon. A
// `fetch`, not a Server Action, for the reason the steer is one: Next runs one
// tab's Server Actions one at a time, so a Stop would queue behind a send.
//
// Every outcome is a `TurnStopResult`, never a throw. A 409 is its own kind:
// the turn finished before the Stop landed, so its answer stands and the
// conversation already shows it.

import type { TurnDetail } from "./events";
import { fleetTurnDetailUrl } from "./events-types";

/** The one status a Stop writes (`PATCH …/turns/{turn_id}`). */
export const TURN_STOP_STATUS = "cancelled";

/** Bounded so a hung proxy never leaves Stop pressed forever. */
const STOP_TIMEOUT_MS = 30_000;
const HTTP_CONFLICT = 409;
const CONTENT_TYPE_JSON = "application/json";

export const TURN_STOP_KIND = {
  /** The daemon cancelled the turn; `turn` is its saved row. */
  STOPPED: "stopped",
  /** The turn was answered or failed first; its answer stands. */
  FINISHED: "finished",
  /** No answer, or a refusal: the turn's state is unknown here. */
  FAILED: "failed",
} as const;

export type TurnStopResult =
  | { kind: typeof TURN_STOP_KIND.STOPPED; turn: TurnDetail }
  | { kind: typeof TURN_STOP_KIND.FINISHED }
  | { kind: typeof TURN_STOP_KIND.FAILED; status?: number };

/** Asks the daemon to cancel `turnId` and the run serving it. */
export async function stopTurn(
  workspaceId: string,
  fleetId: string,
  turnId: string,
  signal: AbortSignal,
): Promise<TurnStopResult> {
  let res: Response;
  try {
    res = await fetch(fleetTurnDetailUrl(workspaceId, fleetId, turnId), {
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
  const body: unknown = await res.json().catch(() => null);
  return isTurn(body) ? { kind: TURN_STOP_KIND.STOPPED, turn: body } : { kind: TURN_STOP_KIND.FAILED };
}

// The fields the stream settles a row by; the rest is the daemon's to shape.
function isTurn(body: unknown): body is TurnDetail {
  return typeof body === "object" && body !== null
    && "turn_id" in body && typeof body.turn_id === "string"
    && "status" in body && typeof body.status === "string";
}
