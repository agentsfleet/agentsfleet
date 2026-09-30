import { capturedRun, ev, mockStream, postSteerMock, renderThread } from "./harness";
import { ACCEPTED, OPERATION_ID, composerInput, send } from "./steer-helpers";
import { SEND_LABEL } from "./steer-copy";
import { describe, expect, it } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { AGENTSFLEET_EVENT_STATUS, type FleetEventStatus } from "@/lib/streaming/fleet-stream-row";

// The steer rides assistant-ui's queue: the thread reports a running reply as
// running, and Send stays open through it because the queue takes every send
// to the one delivery path.

const RUNNING_STATUS = AGENTSFLEET_EVENT_STATUS.RECEIVED;
const SETTLED_STATUS = AGENTSFLEET_EVENT_STATUS.PROCESSED;

function turn(status: FleetEventStatus) {
  return ev({ id: `evt_${status}`, role: "user", actor: "operator", text: "Run it", status });
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
    renderThread();
    expect(capturedRun.isRunning).toBe(false);
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
