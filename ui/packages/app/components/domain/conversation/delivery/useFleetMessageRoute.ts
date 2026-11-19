import { PENDING_SEND_STATE, type PendingSendOutcome } from "./useFleetPendingSends";
import { parseChatAddress } from "@/lib/chat-address";
import { HTTP_STATUS_UNAUTHORIZED, isDefiniteRefusal } from "@/lib/api/errors";
import { findFleetByName } from "@/lib/api/fleet-by-name";
import type { SteerResult } from "@/lib/api/fleet-steer";
import { ERROR_CODE } from "@/lib/errors";

/** The open fleet: where a message with no address, or one addressed by its own name, goes. */
type Here = { workspaceId: string; fleetId: string; fleetName: string };
/** Where a send's words go, or how it ends without posting them. */
export type Route = { ok: true; fleetId: string; words: string } | { ok: false; outcome: PendingSendOutcome };

// The fleet an addressed operation resolved to, in module state like the
// delivery queues. A send that may have committed is sent again under the same
// operation id, and the daemon keys that id by its target fleet: a name that
// resolved elsewhere by then would run the words a second time on another
// fleet. So every retry of an operation goes where its first attempt resolved,
// until the daemon accepts it.
const RESOLVED = new Map<string, string>();
const pinKey = (workspaceId: string, operationId: string): string => `${workspaceId}:${operationId}`;

/** Ends an operation's pin: the daemon holds the send, so nothing retries it. */
export function releaseRoute(workspaceId: string, operationId: string): void {
  RESOLVED.delete(pinKey(workspaceId, operationId));
}

// An address with no words after it has nothing to send, so it ends refused
// before any lookup. The open fleet's own name resolves here. Any other name
// resolves first, and one no fleet is saved under ends refused with nothing
// posted.
export async function routeOf(text: string, here: Here, operationId: string, signal: AbortSignal): Promise<Route> {
  const address = parseChatAddress(text);
  if (address === null) return { ok: true, fleetId: here.fleetId, words: text };
  if (address.rest.length === 0) return { ok: false, outcome: PENDING_SEND_STATE.REFUSED };
  const key = pinKey(here.workspaceId, operationId);
  const pinned = RESOLVED.get(key);
  if (pinned !== undefined) return { ok: true, fleetId: pinned, words: address.rest };
  if (address.name === here.fleetName) {
    RESOLVED.set(key, here.fleetId);
    return { ok: true, fleetId: here.fleetId, words: address.rest };
  }
  const lookup = await findFleetByName(here.workspaceId, address.name, signal);
  if (!lookup.ok) return { ok: false, outcome: outcomeOf(lookup) };
  if (lookup.fleetId === null) return { ok: false, outcome: PENDING_SEND_STATE.REFUSED };
  RESOLVED.set(key, lookup.fleetId);
  return { ok: true, fleetId: lookup.fleetId, words: address.rest };
}

// A 401 asks for a sign-in. A reused id is refused for good. Any other client
// refusal but a timeout means the server saw the request and said no. Anything
// else — a timeout, a 5xx after the row committed, no status at all — leaves
// delivery unconfirmed. A failed name lookup ends the same way, with nothing
// posted.
export function outcomeOf({ status, errorCode }: Pick<Extract<SteerResult, { ok: false }>, "status" | "errorCode">): PendingSendOutcome {
  if (status === HTTP_STATUS_UNAUTHORIZED) return PENDING_SEND_STATE.SESSION;
  if (errorCode === ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT) return PENDING_SEND_STATE.CONFLICT;
  return isDefiniteRefusal(status) ? PENDING_SEND_STATE.REFUSED : PENDING_SEND_STATE.UNKNOWN;
}
