import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

/**
 * Turns under this tab's own account whose tab it cannot tell yet.
 *
 * A steer's opening frame names the sender's account, never the tab, and the
 * daemon can lease a send and announce it before the send's own 202 reaches
 * this tab. While a send here awaits its 202, a turn the thread has no row for,
 * opened under the account that send will be named under (`sentAs`), may be
 * this tab's or the same operator's in another tab, so its frames wait here
 * until the 202 says which. The turn the 202 names lands on this tab's row,
 * which carries the submit clock that makes a run this tab's (`reportsOwnRun`);
 * any other lands as its own row once no send here is waiting. A teammate's,
 * the API's or a webhook's turn is never held. Every send ends in a 202 or a
 * discard inside its deadline, so nothing waits longer than that.
 */
export class HeldTurns {
  #frames = new Map<string, LiveFrame[]>();

  /** Takes `frame` when its turn is held, or starts holding a turn it opens. */
  take(frame: LiveFrame, events: readonly FleetEvent[]): boolean {
    const eventId = "event_id" in frame ? frame.event_id : null;
    if (eventId === null) return false;
    const held = this.#frames.get(eventId);
    if (held !== undefined) {
      held.push(frame);
      return true;
    }
    if (!opensAWaitingSendsTurn(frame, events)) return false;
    this.#frames.set(eventId, [frame]);
    return true;
  }

  /** The frames now free to land, each turn's in arrival order: a turn the
   * thread holds a row for (the 202 named it), and every turn once no send
   * here awaits its 202. */
  release(events: readonly FleetEvent[]): LiveFrame[] {
    const waiting = events.some(isAwaitingAck);
    return this.#take((eventId) => !waiting || events.some((event) => event.id === eventId));
  }

  /** The frames of the held turns among `eventIds`, which a backfill page is
   * about to settle. */
  releaseTurns(eventIds: ReadonlySet<string>): LiveFrame[] {
    return this.#take((eventId) => eventIds.has(eventId));
  }

  #take(free: (eventId: string) => boolean): LiveFrame[] {
    const frames: LiveFrame[] = [];
    for (const [eventId, held] of this.#frames) {
      if (!free(eventId)) continue;
      this.#frames.delete(eventId);
      frames.push(...held);
    }
    return frames;
  }
}

// Either frame can open a turn: the admitted frame normally comes first, but a
// runner can lease a send and announce it before the route publishes.
function opensAWaitingSendsTurn(frame: LiveFrame, events: readonly FleetEvent[]): boolean {
  if (frame.kind !== FRAME_KIND.EVENT_ADMITTED && frame.kind !== FRAME_KIND.EVENT_RECEIVED) return false;
  return !events.some((event) => event.id === frame.event_id)
    && events.some((event) => isAwaitingAck(event) && event.sentAs === frame.actor);
}

function isAwaitingAck(event: FleetEvent): boolean {
  return event.status === AGENTSFLEET_EVENT_STATUS.OPTIMISTIC;
}
