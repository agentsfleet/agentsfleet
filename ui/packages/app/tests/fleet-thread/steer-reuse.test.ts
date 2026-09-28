import { SUBJECT, WS, ZID, mockStream, renderThread, steerFleetActionMock } from "./harness";
import {
  ACCEPTED, REFUSED, RESEND_LABEL, SEND_LABEL, UNAVAILABLE, composerInput, heldRefusal, operationIdOf, send,
} from "./steer-helpers";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { PENDING_SEND_STATE, getPendingSends } from "@/lib/streaming/pending-sends";

// Which send an operation id belongs to. Only a failed send's own draft —
// returned by assistant-ui, or restored from the ledger on a remount — sent
// unchanged, reuses that send's id. New words get a new one, even the same
// words: an old id would replay an old admission and run nothing.

const SCOPE = { subject: SUBJECT, workspaceId: WS, fleetId: ZID };

describe("FleetThread — operation id reuse", () => {
  it("gives the same words a new id when a newer send kept the failed draft out", async () => {
    const held = heldRefusal();
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_deploy")).mockResolvedValueOnce(ACCEPTED("evt_yes"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_any") });
    renderThread();
    await send("yes");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(1));
    await send("deploy");
    await act(async () => {
      held.refuse();
    });
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    // "deploy" was the newer send, so assistant-ui kept "yes" out of the composer.
    expect(composerInput().value).toBe("");
    await send("yes");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(3));
    expect(operationIdOf(2)).not.toBe(operationIdOf(0));
  });

  it("sends a draft restored on a remount under the send's own id", async () => {
    steerFleetActionMock.mockResolvedValueOnce(UNAVAILABLE).mockResolvedValueOnce(ACCEPTED("evt_once"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_restored") });
    const view = renderThread();
    await send("restore me");
    await waitFor(() => expect(getPendingSends(SCOPE).map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.UNKNOWN]));
    view.unmount();
    renderThread();
    await waitFor(() => expect(composerInput().value).toBe("restore me"));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: SEND_LABEL }));
    });
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).toBe(operationIdOf(0));
  });

  it("gives the same words typed after a landed Resend a new id", async () => {
    steerFleetActionMock
      .mockResolvedValueOnce(REFUSED)
      .mockResolvedValueOnce(ACCEPTED("evt_resent"))
      .mockResolvedValueOnce(ACCEPTED("evt_new"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_any") });
    renderThread();
    await send("yes");
    await waitFor(() => expect(composerInput().value).toBe("yes"));
    fireEvent.click(screen.getByRole("button", { name: RESEND_LABEL }));
    await waitFor(() => expect(getPendingSends(SCOPE)).toEqual([]));
    await send("yes");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(3));
    expect(operationIdOf(1)).toBe(operationIdOf(0));
    expect(operationIdOf(2)).not.toBe(operationIdOf(0));
  });

  it("gives a returned draft edited away and back a new id", async () => {
    steerFleetActionMock.mockResolvedValueOnce(REFUSED).mockResolvedValueOnce(ACCEPTED("evt_edited"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_any") });
    renderThread();
    await send("yes");
    await waitFor(() => expect(composerInput().value).toBe("yes"));
    fireEvent.change(composerInput(), { target: { value: "ye" } });
    await send("yes");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).not.toBe(operationIdOf(0));
  });

  it("keeps the fleet's queue moving after a send throws past its acknowledgement", async () => {
    const reconcileOptimistic = vi.fn().mockImplementationOnce(() => {
      throw new Error("paint failed");
    });
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_any"), reconcileOptimistic });
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_first")).mockResolvedValueOnce(ACCEPTED("evt_second"));
    renderThread();
    await send("first");
    await waitFor(() => expect(reconcileOptimistic).toHaveBeenCalledTimes(1));
    // The daemon holds the first message: its ledger entry settled before the throw.
    expect(getPendingSends(SCOPE)).toEqual([]);
    await send("second");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
  });
});
