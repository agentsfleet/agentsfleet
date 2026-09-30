import { describe, expect, it } from "vitest";
import { type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ACTOR } from "@/lib/events/event-summary";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";
import { HeldTurns } from "./fleet-stream-held";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

const OWN = `${ACTOR.STEER_PREFIX}user_viewer`;
const MINE = "evt_mine";
const ELSEWHERE = "evt_elsewhere";
const WAITING: FleetEvent[] = [evt({ id: "optim-1", role: "user", actor: ACTOR.PENDING_STEER, status: AGENTSFLEET_EVENT_STATUS.OPTIMISTIC })];
const NAMED_MINE: FleetEvent[] = [evt({ id: MINE, role: "user", actor: ACTOR.PENDING_STEER, status: AGENTSFLEET_EVENT_STATUS.RECEIVED })];
const IDLE: FleetEvent[] = [];

function opening(eventId: string, actor = OWN): LiveFrame {
  return { kind: FRAME_KIND.EVENT_RECEIVED, event_id: eventId, actor };
}

function tool(eventId: string): LiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: eventId, name: "read_file", args_redacted: true };
}

describe("HeldTurns", () => {
  it("test_holds_a_steer_turn_while_a_send_here_waits", () => {
    const held = new HeldTurns();
    expect(held.take(opening(MINE), WAITING)).toBe(true);
    // Every later frame of a held turn waits behind its opening.
    expect(held.take(tool(MINE), WAITING)).toBe(true);
  });

  it("test_lets_every_other_frame_through", () => {
    const held = new HeldTurns();
    // Nothing of this tab's is waiting on a 202.
    expect(held.take(opening(MINE), IDLE)).toBe(false);
    // A fleet or webhook turn is never a send from a tab.
    expect(held.take(opening(MINE, ACTOR.FLEET), WAITING)).toBe(false);
    // The thread already holds this turn's row.
    expect(held.take(opening(MINE), NAMED_MINE)).toBe(false);
    // Activity for a turn that was never held.
    expect(held.take(tool(ELSEWHERE), WAITING)).toBe(false);
    // A frame that names no turn.
    expect(held.take({ kind: FRAME_KIND.CATCHING_UP, dropped: 1 }, WAITING)).toBe(false);
  });

  it("test_the_202_releases_the_turn_it_names", () => {
    const held = new HeldTurns();
    held.take(opening(MINE), WAITING);
    held.take(tool(MINE), WAITING);
    held.take(opening(ELSEWHERE), WAITING);
    // The 202 named evt_mine while a second send still waits: only its turn
    // lands, in arrival order, and the other keeps waiting.
    const stillWaiting = [...NAMED_MINE, ...WAITING];
    expect(held.release(stillWaiting)).toEqual([opening(MINE), tool(MINE)]);
    expect(held.release(stillWaiting)).toEqual([]);
    // No send here waits any more: the rest is another tab's, and lands.
    expect(held.release(NAMED_MINE)).toEqual([opening(ELSEWHERE)]);
    expect(held.release(NAMED_MINE)).toEqual([]);
  });

  it("test_a_discard_releases_everything", () => {
    const held = new HeldTurns();
    held.take(opening(ELSEWHERE), WAITING);
    expect(held.release(IDLE)).toEqual([opening(ELSEWHERE)]);
  });
});
