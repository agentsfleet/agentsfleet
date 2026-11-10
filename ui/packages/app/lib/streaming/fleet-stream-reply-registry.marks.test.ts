import { afterEach, describe, expect, it, vi } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { IN_ORDER, chunkOf, completionOf, markedSends } from "@/tests/helpers/send-marks-fixtures";
import { WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { createEntry } from "./fleet-stream-entry";
import type { FleetEvent } from "./fleet-stream-row";
import { dispatchReplyFrame } from "./fleet-stream-reply-registry";
import { SEND_MARK, SEND_MARKS_MAX, sendMarks } from "./send-marks";

const OPERATION_ID = "0190a3b4-0000-7000-8000-000000000001";
const EVENT_ID = "1790573387481-566";

afterEach(() => {
  performance.clearMarks();
  sendMarks.reset();
});

describe("a send's marks through the live stream", () => {
  it("test_dispatched_frames_mark_an_accepted_send: the stream's own dispatch leaves the first-frame and completion marks", () => {
    sendMarks.mark(SEND_MARK.SEND, OPERATION_ID);
    sendMarks.mark(SEND_MARK.ACCEPTED, OPERATION_ID, EVENT_ID);
    const entry = createEntry(WS, []);
    let events: FleetEvent[] = [];
    const apply = vi.fn((next: (prev: FleetEvent[]) => FleetEvent[]) => { events = next(events); });
    for (const frame of [chunkOf(EVENT_ID), completionOf(EVENT_ID)]) dispatchReplyFrame(entry, Z_A, frame, apply, () => true);
    expect(markedSends()).toEqual(IN_ORDER.map((name) => [name, OPERATION_ID]));
  });

  it("forgets the oldest accepted send past the cap, so its late frames leave no mark", () => {
    const eventOf = (n: number) => `capped-event-${n}`;
    for (let n = 0; n <= SEND_MARKS_MAX; n++) sendMarks.mark(SEND_MARK.ACCEPTED, `capped-op-${n}`, eventOf(n));
    performance.clearMarks();
    sendMarks.frame(completionOf(eventOf(0)));
    sendMarks.frame(completionOf(eventOf(SEND_MARKS_MAX)));
    expect(markedSends().map(([, id]) => id)).toEqual([`capped-op-${SEND_MARKS_MAX}`, `capped-op-${SEND_MARKS_MAX}`]);
  });

  it("leaves no frame mark for a frame that names no event, or an event no send here was accepted under", () => {
    sendMarks.frame({ kind: FRAME_KIND.HELLO, fleet_ids: [] });
    sendMarks.frame({ kind: FRAME_KIND.GATE_RESOLVED, gate_id: "gate_1", event_id: null, status: "approved", resolved_by: "someone", pending_approvals: 0 });
    sendMarks.frame(chunkOf("someone-elses-event"));
    sendMarks.frame(completionOf("someone-elses-event"));
    expect(markedSends()).toEqual([]);
  });
});
