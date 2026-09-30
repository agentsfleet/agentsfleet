import { describe, expect, it } from "vitest";
import type { LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ACTOR } from "@/lib/events/event-summary";
import { convertEvent } from "@/components/domain/useFleetEventStream";
import { toReplyMessage } from "@/components/domain/fleetReplyMessage";
import { applyLiveFrame } from "./fleet-stream-frames";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";

const EVENT_ID = "1725000000000-3";
const TEAMMATE = `${ACTOR.STEER_PREFIX}user_bob`;
const TYPED = "check the tests";
const ADMITTED_AT = 1_725_000_000_000;
const RECEIVED_AT = ADMITTED_AT + 5_000;

function admitted(message: string | null = TYPED): LiveFrame {
  return {
    kind: FRAME_KIND.EVENT_ADMITTED,
    event_id: EVENT_ID,
    actor: TEAMMATE,
    event_type: "chat",
    ...(message === null ? {} : { message }),
    created_at: ADMITTED_AT,
  };
}

function received(message?: string): LiveFrame {
  return {
    kind: FRAME_KIND.EVENT_RECEIVED,
    event_id: EVENT_ID,
    actor: TEAMMATE,
    event_type: "chat",
    created_at: RECEIVED_AT,
    ...(message === undefined ? {} : { message }),
  };
}

function fold(frames: LiveFrame[]): FleetEvent[] {
  return frames.reduce<FleetEvent[]>((rows, frame) => applyLiveFrame(rows, frame), []);
}

describe("a message waiting for a runner", () => {
  it("test_admitted_frame_renders_queued_row", () => {
    const [row, ...rest] = fold([admitted()]);
    expect(rest).toEqual([]);
    expect(row).toMatchObject({
      id: EVENT_ID,
      role: "user",
      actor: TEAMMATE,
      text: TYPED,
      status: AGENTSFLEET_EVENT_STATUS.QUEUED,
    });
    expect(row?.createdAt.getTime()).toBe(ADMITTED_AT);
    // It renders as a turn whose reply is queued: running, marked queued.
    const message = convertEvent(row as FleetEvent);
    expect(message.metadata?.custom?.queued).toBe(true);
    expect(toReplyMessage(message, row as FleetEvent).status).toEqual({ type: "running" });
  });

  it("test_received_moves_queued_row", () => {
    const rows = fold([admitted(), received(TYPED)]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_EVENT_STATUS.RECEIVED });
    expect(rows[0]?.createdAt.getTime()).toBe(RECEIVED_AT);
    expect(convertEvent(rows[0] as FleetEvent).metadata?.custom?.queued).toBe(false);
  });

  it("test_late_admitted_frame_keeps_status", () => {
    // A runner leased the send and announced it before the route published.
    const rows = fold([received(), admitted()]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_EVENT_STATUS.RECEIVED });
  });

  it("keeps a row's own text over a late admitted frame's", () => {
    const rows = fold([received("what the row already says"), admitted()]);
    expect(rows[0]?.text).toBe("what the row already says");
  });

  it("names the typed words on a received frame that opens the row", () => {
    const rows = fold([received(TYPED)]);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_EVENT_STATUS.RECEIVED });
  });

  it("opens a row with no text when the frame carried none", () => {
    expect(fold([admitted(null)])[0]?.text).toBe("");
  });
});
