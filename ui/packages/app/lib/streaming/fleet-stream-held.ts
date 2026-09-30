import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ACTOR } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

/**
 * A steer's turns whose sender this tab cannot tell yet.
 *
 * A steer's opening frame names the sender's account, never the tab, and the
 * daemon can lease a send and announce it before the send's own 202 reaches
 * this tab. While a send here awaits its 202, a steer's turn the thread does
 * not hold may be this tab's or another tab's, so its frames wait here until
 * the 202 says which. The turn the 202 names lands on this tab's row, which
 * carries the submit clock that makes a run this tab's (`reportsOwnRun`); any
 * other lands as its own row once no send here is waiting. Every send ends in
 * a 202 or a discard inside its deadline, so nothing waits longer than that.
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
    if (!opensUnheldSteer(frame, events) || !events.some(isAwaitingAck)) return false;
    this.#frames.set(eventId, [frame]);
    return true;
  }

  /** The frames now free to land, each turn's in arrival order: a turn the
   * thread holds a row for (the 202 named it), and every turn once no send
   * here awaits its 202. */
  release(events: readonly FleetEvent[]): LiveFrame[] {
    const waiting = events.some(isAwaitingAck);
    const free: LiveFrame[] = [];
    for (const [eventId, frames] of this.#frames) {
      if (waiting && !events.some((event) => event.id === eventId)) continue;
      this.#frames.delete(eventId);
      free.push(...frames);
    }
    return free;
  }
}

function opensUnheldSteer(frame: LiveFrame, events: readonly FleetEvent[]): boolean {
  return frame.kind === FRAME_KIND.EVENT_RECEIVED
    && frame.actor.startsWith(ACTOR.STEER_PREFIX)
    && !events.some((event) => event.id === frame.event_id);
}

function isAwaitingAck(event: FleetEvent): boolean {
  return event.status === AGENTSFLEET_EVENT_STATUS.OPTIMISTIC;
}
