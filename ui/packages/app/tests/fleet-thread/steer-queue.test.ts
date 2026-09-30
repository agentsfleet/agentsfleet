import { SUBJECT, capturedRun, ev, mockStream, postSteerMock, renderThread } from "./harness";
import { ACCEPTED, OPERATION_ID, composerInput, send } from "./steer-helpers";
import { SEND_LABEL } from "./steer-copy";
import { describe, expect, it } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { ACTOR } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS, type FleetEventStatus } from "@/lib/streaming/fleet-stream-row";

// The steer rides assistant-ui's queue: the thread reports the viewer's own
// running reply as running, and Send stays open through it because the queue
// takes every send to the one delivery path.

const RUNNING_STATUS = AGENTSFLEET_EVENT_STATUS.RECEIVED;
const SETTLED_STATUS = AGENTSFLEET_EVENT_STATUS.PROCESSED;
const OWN_ACTOR = `${ACTOR.STEER_PREFIX}${SUBJECT}`;
const TEAMMATE_ACTOR = `${ACTOR.STEER_PREFIX}user_teammate`;

function turn(status: FleetEventStatus, actor = OWN_ACTOR) {
  return ev({ id: `evt_${status}`, role: "user", actor, text: "Run it", status });
}

describe("FleetThread — steer queue", () => {
  it("test_is_running_tracks_reply_rows", () => {
    mockStream([]);
    const idle = renderThread();
    expect(capturedRun.isRunning).toBe(false);
    expect(capturedRun.hasQueue).toBe(true);
    idle.unmount();

    mockStream([turn(RUNNING_STATUS)]);
    const running = renderThread();
    expect(capturedRun.isRunning).toBe(true);
    running.unmount();

    mockStream([turn(SETTLED_STATUS)]);
    const settled = renderThread();
    expect(capturedRun.isRunning).toBe(false);
    settled.unmount();

    // Another sender's run never reports: the viewport's top anchor keys on
    // it, and would pull a reader out of the history to that turn.
    mockStream([turn(RUNNING_STATUS, TEAMMATE_ACTOR), turn(RUNNING_STATUS, ACTOR.API_STEER)]);
    const others = renderThread();
    expect(capturedRun.isRunning).toBe(false);
    others.unmount();

    // A send from this tab the daemon has not named yet is the viewer's own.
    mockStream([turn(RUNNING_STATUS, ACTOR.PENDING_STEER)]);
    renderThread();
    expect(capturedRun.isRunning).toBe(true);
  });

  it("test_send_enabled_while_running", () => {
    mockStream([turn(RUNNING_STATUS)]);
    renderThread();
    expect(capturedRun.isRunning).toBe(true);
    // A draft to send, so only the run could hold Send closed.
    fireEvent.change(composerInput(), { target: { value: "one more thing" } });
    const sendButton = screen.getByRole("button", { name: SEND_LABEL }) as HTMLButtonElement;
    expect(sendButton.disabled).toBe(false);
  });

  it("test_queue_send_posts_once", async () => {
    postSteerMock.mockResolvedValueOnce(ACCEPTED("evt_a")).mockResolvedValueOnce(ACCEPTED("evt_b"));
    mockStream([turn(RUNNING_STATUS)]);
    renderThread();
    await send("steer while running");
    await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(1));
    await send("and another");
    await waitFor(() => expect(postSteerMock).toHaveBeenCalledTimes(2));
    const [first, second] = postSteerMock.mock.calls.map((call) => call[3] as string);
    expect(first).toEqual(OPERATION_ID);
    expect(second).toEqual(OPERATION_ID);
    expect(first).not.toBe(second);
    expect(postSteerMock.mock.calls.map((call) => call[2])).toEqual(["steer while running", "and another"]);
  });
});
