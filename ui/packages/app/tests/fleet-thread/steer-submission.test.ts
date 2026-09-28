import { SUBJECT, WS, ZID, appendMessage, capturedOnNew, capturedSubmittedMessageId, ev, mockStream, renderThread, steerFleetActionMock, threadElement } from "./harness";
import { ACCEPTED, OPERATION_ID, REFUSED, TOO_LONG_TEXT, UUID_V7, composerInput, heldRefusal, operationIdOf, send } from "./steer-helpers";
import { SEND_LABEL } from "./steer-copy";
import { STEER_MESSAGE_MAX_BYTES } from "@/lib/api/fleets-types";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import type { AppendMessage } from "@assistant-ui/react";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import { getPendingSends } from "@/lib/streaming/pending-sends";
import { MintUnavailable } from "@/lib/streaming/operation-id";

// The real mint by default; a case that needs to see it, or fail it, overrides
// one call.
const { mintMock, actualMint } = vi.hoisted(() => ({
  mintMock: vi.fn<() => string>(),
  actualMint: { current: (): string => "" },
}));
vi.mock("@/lib/streaming/operation-id", async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/streaming/operation-id")>();
  actualMint.current = real.mintOperationId;
  mintMock.mockImplementation(real.mintOperationId);
  return { ...real, mintOperationId: mintMock };
});

afterEach(() => {
  mintMock.mockImplementation(actualMint.current);
});

describe("FleetThread — steer submission", () => {
  it("serialises rapid submissions so their HTTP requests reach the server in order", async () => {
    mockStream([]);
    // Two rapid sends: the first resolves slowly, the second quickly. Without
    // serialisation the fast one's HTTP request would land first and the server would
    // assign it the earlier event id — "stop" before "deploy".
    const order: string[] = [];
    let releaseFirst: (() => void) | null = null;
    steerFleetActionMock.mockImplementationOnce(
      (_ws: string, _z: string, text: string) =>
        new Promise((resolve) => {
          releaseFirst = () => {
            order.push(text);
            resolve(ACCEPTED("evt_1"));
          };
        }),
    );
    steerFleetActionMock.mockImplementationOnce(
      (_ws: string, _z: string, text: string) => {
        order.push(text);
        return Promise.resolve(ACCEPTED("evt_2"));
      },
    );
    renderThread();

    const first = capturedOnNew.current!(appendMessage("deploy"));
    const second = capturedOnNew.current!(appendMessage("stop"));
    // The second HTTP request must not fire until the first resolves.
    await waitFor(() => expect(releaseFirst).not.toBeNull());
    expect(order).toEqual([]);
    await act(async () => {
      releaseFirst!();
      await Promise.all([first, second]);
    });
    expect(order).toEqual(["deploy", "stop"]);
    // Two operations, two names.
    expect(operationIdOf(0)).not.toBe(operationIdOf(1));
  });

  it("calls steerFleetAction with a named operation and reconciles the optimistic message on ok", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_42");
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, discardOptimistic });
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_real_42"));
    renderThread();
    await capturedOnNew.current!(appendMessage("deploy the canary"));
    await waitFor(() =>
      expect(steerFleetActionMock).toHaveBeenCalledWith(WS, ZID, "deploy the canary", OPERATION_ID),
    );
    expect(appendOptimistic).toHaveBeenCalledWith("deploy the canary", "steer:pending");
    expect(reconcileOptimistic).toHaveBeenCalledWith("temp_42", "evt_real_42", false);
    expect(discardOptimistic).not.toHaveBeenCalled();
    expect(refreshed).toHaveBeenCalledTimes(1);
    // Acknowledged: nothing is left to recover.
    expect(getPendingSends({ subject: SUBJECT, workspaceId: WS, fleetId: ZID })).toEqual([]);
    unsubscribe();
  });

  it("test_operation_id_minted_before_append", async () => {
    const order: string[] = [];
    const appendOptimistic = vi.fn(() => {
      order.push("append");
      return "temp_o";
    });
    mockStream([], { appendOptimistic });
    steerFleetActionMock.mockImplementationOnce(async () => {
      order.push("action");
      return ACCEPTED("evt_o");
    });
    mintMock.mockImplementationOnce(() => {
      order.push("mint");
      return actualMint.current();
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("name me first"));
    expect(order).toEqual(["mint", "append", "action"]);
    expect(operationIdOf(0)).toMatch(UUID_V7);
  });

  it("test_mint_sorts_by_time_then_refuses", async () => {
    // No generator on this platform: the send is refused before anything is
    // appended or posted, and the draft comes back to the composer.
    const appendOptimistic = vi.fn();
    mockStream([], { appendOptimistic });
    mintMock.mockImplementationOnce(() => {
      throw new MintUnavailable();
    });
    renderThread();
    await send("cannot be named");
    await waitFor(() => expect(composerInput().value).toBe("cannot be named"));
    expect(appendOptimistic).not.toHaveBeenCalled();
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(getPendingSends({ subject: SUBJECT, workspaceId: WS, fleetId: ZID })).toEqual([]);
  });

  it("gives the same words sent again while the first is out their own operation", async () => {
    // "yes", then "yes" again before the first answer: two messages, two ids.
    const held = heldRefusal();
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_second_yes"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValueOnce("temp_yes_1").mockReturnValueOnce("temp_yes_2") });
    renderThread();
    await send("yes");
    await send("yes");
    await act(async () => {
      held.refuse();
    });
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).not.toBe(operationIdOf(0));
  });

  it("never lends an old failed send's id to a new message with the same words", async () => {
    steerFleetActionMock.mockResolvedValueOnce(REFUSED).mockResolvedValueOnce(ACCEPTED("evt_other")).mockResolvedValueOnce(ACCEPTED("evt_new_yes"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_any") });
    renderThread();
    await send("yes");
    await waitFor(() => expect(composerInput().value).toBe("yes"));
    // Something else is sent from the restored draft's place, then "yes" is
    // typed fresh: that is a new message, not the refused one.
    await send("something else");
    await send("yes");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(3));
    expect(operationIdOf(2)).not.toBe(operationIdOf(0));
  });

  it("refuses a draft longer than the daemon takes before it is named or recorded, and says why", async () => {
    const appendOptimistic = vi.fn();
    mockStream([], { appendOptimistic });
    renderThread();
    const oversized = "a".repeat(STEER_MESSAGE_MAX_BYTES + 1);
    fireEvent.change(composerInput(), { target: { value: oversized } });
    // Send is disabled; Enter, which does not go through it, is refused too.
    expect((screen.getByRole("button", { name: SEND_LABEL }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => {
      fireEvent.keyDown(composerInput(), { key: "Enter" });
    });
    await waitFor(() => expect(composerInput().value).toBe(oversized));
    expect(screen.getByText(TOO_LONG_TEXT)).toBeTruthy();
    expect(appendOptimistic).not.toHaveBeenCalled();
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(getPendingSends({ subject: SUBJECT, workspaceId: WS, fleetId: ZID })).toEqual([]);
  });

  it("keeps submit scroll intent through acknowledgement and reordered backfill", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_clock_skew");
    mockStream([], { appendOptimistic });
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_clock_skew"));
    const view = renderThread();

    await act(async () => {
      await capturedOnNew.current!(appendMessage("recent operator message"));
    });
    expect(capturedSubmittedMessageId.current).toBe("temp_clock_skew");

    mockStream([
      ev({ id: "temp_clock_skew", role: "user", actor: "steer:pending", status: "optimistic" }),
      ev({ id: "newer_server_row", role: "system", actor: "webhook", createdAt: new Date("2026-05-15T18:00:00Z") }),
    ], { appendOptimistic });
    view.rerender(threadElement());
    expect(capturedSubmittedMessageId.current).toBe("temp_clock_skew");
  });

  it("accepts a steer that completed before its HTTP response returned", async () => {
    const reconcileOptimistic = vi.fn().mockReturnValue(true);
    mockStream([], { reconcileOptimistic });
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_already_complete"));
    renderThread();
    await capturedOnNew.current!(appendMessage("fast completion"));
    expect(reconcileOptimistic).toHaveBeenCalledWith("temp_1", "evt_already_complete", false);
  });

  it("does not call the action when the submitted message text is empty", async () => {
    const appendOptimistic = vi.fn();
    mockStream([], { appendOptimistic });
    renderThread();
    await capturedOnNew.current!(appendMessage(""));
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });

  it("does not call the action when the append carries no text part", async () => {
    // The composer UI always emits a text part, but `onNew` may receive an
    // image-only append. `extractMessageText` must fall through to "" and the
    // empty-text guard must short-circuit before any optimistic write or remote call.
    const appendOptimistic = vi.fn().mockReturnValue("temp_img");
    mockStream([], { appendOptimistic });
    renderThread();
    expect(capturedOnNew.current).toBeTypeOf("function");
    await capturedOnNew.current!({
      content: [{ type: "image", image: "data:image/png;base64,xx" }],
    } as unknown as AppendMessage);
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });
});
