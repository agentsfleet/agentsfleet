import { SUBJECT, WS, ZID, mockStream, renderThread, steerFleetActionMock } from "./harness";
import {
  ACCEPTED, DISMISS_LABEL, NOTICES_LABEL, REFUSED, RESEND_LABEL, SEND_FAILED_TEXT, SEND_UNCONFIRMED_TEXT, SIGN_IN_LABEL, UNAVAILABLE,
  composerInput, heldRefusal, operationIdOf, send,
} from "./steer-helpers";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import { PENDING_SEND_STATE, dismissPendingSend, getPendingSends } from "@/lib/streaming/pending-sends";

// A send that did not come back: refused, unconfirmed, or refused after the
// composer that sent it was gone. Every case ends in one ledger entry per
// unresolved send, and every Resend carries the operation id it was sent with.

// Longer than any retry schedule the action could run: a replay would land
// inside it.
const NO_REPLAY_WINDOW_MS = 5_000;
const TRANSPORT_FAILED = new Error("Server Component transport failed");
const SCOPE = { subject: SUBJECT, workspaceId: WS, fleetId: ZID };
// The entries the notice can show: a dismissal stays behind as a tombstone.
const noticed = () => getPendingSends(SCOPE).filter((entry) => entry.state !== PENDING_SEND_STATE.DISMISSED);
// Answers that settle nothing: a client timeout, a server error after the row
// may have committed, and a failure with no status at all.
const UNSETTLED_ANSWERS = [
  UNAVAILABLE,
  { ok: false, error: "Request timed out", status: 408, errorCode: "UZ-REQ-408" },
  { ok: false, error: "Something failed", status: undefined, errorCode: undefined },
] as const;

describe("FleetThread — steer recovery", () => {
  it("test_failed_send_leaves_thread_and_restores_draft", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_t"), reconcileOptimistic, discardOptimistic });
    steerFleetActionMock.mockResolvedValueOnce(REFUSED);
    renderThread();
    await send("refused send");
    await waitFor(() => expect(discardOptimistic).toHaveBeenCalledWith("temp_t"));
    // assistant-ui hands the draft back when the handler rejects with
    // MessageNotSentError.
    await waitFor(() => expect(composerInput().value).toBe("refused send"));
    expect(screen.getByText(SEND_FAILED_TEXT)).toBeTruthy();
    expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy();
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    expect(getPendingSends(SCOPE).map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.REFUSED]);
    unsubscribe();
  });

  it("test_unknown_delivery_notice", async () => {
    // The Server Action's transport failed: nothing answered, so the message
    // may or may not have landed. The notice says so, and Resend carries the
    // same operation id, which is what makes it safe to click.
    const discardOptimistic = vi.fn();
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_u"), discardOptimistic });
    steerFleetActionMock.mockRejectedValueOnce(TRANSPORT_FAILED).mockResolvedValueOnce(ACCEPTED("evt_confirmed"));
    renderThread();
    await send("offline send");
    await waitFor(() => expect(discardOptimistic).toHaveBeenCalledWith("temp_u"));
    await waitFor(() => expect(composerInput().value).toBe("offline send"));
    expect(screen.getByText(SEND_UNCONFIRMED_TEXT)).toBeTruthy();
    expect(screen.queryByText(SEND_FAILED_TEXT)).toBeNull();
    expect(getPendingSends(SCOPE).map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.UNKNOWN]);

    fireEvent.click(screen.getByRole("button", { name: RESEND_LABEL }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).toBe(operationIdOf(0));
    await waitFor(() => expect(screen.queryByText(SEND_UNCONFIRMED_TEXT)).toBeNull());
    expect(getPendingSends(SCOPE)).toEqual([]);
  });

  it("test_resend_submits_restored_text_once", async () => {
    const discardOptimistic = vi.fn();
    const reconcileOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic: vi.fn().mockReturnValueOnce("temp_fail_1").mockReturnValueOnce("temp_resend"),
      discardOptimistic,
      reconcileOptimistic,
    });
    steerFleetActionMock.mockResolvedValueOnce(REFUSED).mockResolvedValueOnce(ACCEPTED("evt_resend_ok"));
    // A refused send stays refused until the operator acts. The clock is fake
    // from before the send, so a replay timer armed by the refusal would fire
    // inside the window below.
    vi.useFakeTimers();
    try {
      renderThread();
      await send("retry this send");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(NO_REPLAY_WINDOW_MS);
      });
      expect(composerInput().value).toBe("retry this send");
      expect(steerFleetActionMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }

    fireEvent.click(screen.getByRole("button", { name: RESEND_LABEL }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    // The ledger record, under its own id — never a second operation for the
    // same words.
    expect(steerFleetActionMock).toHaveBeenLastCalledWith(WS, ZID, "retry this send", operationIdOf(0));
    await waitFor(() => expect(reconcileOptimistic).toHaveBeenCalledWith("temp_resend", "evt_resend_ok", false));
    expect(screen.queryByText(SEND_FAILED_TEXT)).toBeNull();
    // Resend cleared the draft that was exactly the refused text.
    expect(composerInput().value).toBe("");
    expect(discardOptimistic).toHaveBeenCalledTimes(1);
  });

  it("test_draft_matching_pending_entry_reuses_its_id", async () => {
    mockStream([], { appendOptimistic: vi.fn().mockReturnValueOnce("temp_old").mockReturnValueOnce("temp_again") });
    steerFleetActionMock.mockResolvedValueOnce(REFUSED).mockResolvedValueOnce(ACCEPTED("evt_old_ok"));
    renderThread();
    await send("old");
    await waitFor(() => expect(composerInput().value).toBe("old"));
    // The operator presses Send on the restored text instead of Resend: the
    // draft IS the unresolved send, so it keeps the id the daemon may hold.
    await send("old");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).toBe(operationIdOf(0));
    await waitFor(() => expect(getPendingSends(SCOPE)).toEqual([]));
  });

  it("test_failure_restore_respects_existing_draft", async () => {
    const held = heldRefusal();
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_old") });
    const view = renderThread();
    await send("old");
    // The POST is chained behind the delivery tail; refuse it once it is out.
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(1));
    fireEvent.change(composerInput(), { target: { value: "new" } });
    await act(async () => {
      held.refuse();
    });
    // Nothing the operator typed since is lost: the refused text returns
    // ahead of it.
    await waitFor(() => expect(composerInput().value).toBe("old\nnew"));
    expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy();

    // A remount starts from an empty composer and gets the refused text back.
    view.unmount();
    renderThread();
    await waitFor(() => expect(composerInput().value).toBe("old"));
  });

  it("test_two_refused_sends_keep_two_entries", async () => {
    const held = heldRefusal();
    steerFleetActionMock.mockResolvedValueOnce(REFUSED);
    mockStream([], { appendOptimistic: vi.fn().mockReturnValueOnce("temp_a").mockReturnValueOnce("temp_b") });
    renderThread();
    await send("first message");
    await send("second message");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(1));
    await act(async () => {
      held.refuse();
    });
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    // The library returns only the latest draft; the ledger keeps both, each
    // with its own Resend.
    await waitFor(() => expect(screen.getAllByRole("button", { name: RESEND_LABEL })).toHaveLength(2));
    // Scoped to the notice: the composer's textarea mirrors its draft into
    // its own text, and the latest draft is the second message.
    const notices = within(screen.getByRole("list", { name: NOTICES_LABEL }));
    expect(notices.getByText("first message")).toBeTruthy();
    expect(notices.getByText("second message")).toBeTruthy();
    expect(getPendingSends(SCOPE).map((entry) => entry.text)).toEqual(["first message", "second message"]);

    // Dismiss takes one entry and nothing else.
    const draftBefore = composerInput().value;
    const [dismissFirst] = screen.getAllByRole("button", { name: DISMISS_LABEL });
    fireEvent.click(dismissFirst as HTMLElement);
    await waitFor(() => expect(screen.getAllByRole("button", { name: RESEND_LABEL })).toHaveLength(1));
    expect(noticed().map((entry) => entry.text)).toEqual(["second message"]);
    expect(composerInput().value).toBe(draftBefore);
  });

  it("test_failure_after_remount_is_resendable", async () => {
    const held = heldRefusal();
    steerFleetActionMock.mockResolvedValueOnce(ACCEPTED("evt_after_remount"));
    mockStream([], { appendOptimistic: vi.fn().mockReturnValueOnce("temp_gone").mockReturnValueOnce("temp_back") });
    const view = renderThread();
    await send("sent then navigated away");
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(1));
    view.unmount();
    renderThread();
    // The refusal lands after the remount: the old composer got the
    // library's draft return, the new one reads the ledger.
    await act(async () => {
      held.refuse();
    });
    await waitFor(() => expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy());
    expect(within(screen.getByRole("list", { name: NOTICES_LABEL })).getByText("sent then navigated away")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: RESEND_LABEL }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(operationIdOf(1)).toBe(operationIdOf(0));
  });

  it("reads an answer that settles nothing as unconfirmed, never as refused", async () => {
    for (const answer of UNSETTLED_ANSWERS) {
      steerFleetActionMock.mockReset();
      steerFleetActionMock.mockResolvedValueOnce(answer);
      mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_x") });
      const view = renderThread();
      await send("maybe landed");
      await waitFor(() => expect(screen.getByText(SEND_UNCONFIRMED_TEXT)).toBeTruthy());
      expect(screen.queryByText(SEND_FAILED_TEXT)).toBeNull();
      expect(noticed().map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.UNKNOWN]);
      view.unmount();
      getPendingSends(SCOPE).forEach((entry) => dismissPendingSend(SCOPE, entry.operationId));
    }
  });

  it("test_session_failure_keeps_sign_in", async () => {
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_99"), reconcileOptimistic, discardOptimistic });
    steerFleetActionMock.mockResolvedValueOnce({ ok: false, error: "Not authenticated", status: 401, errorCode: "UZ-AUTH-401" });
    renderThread();
    await send("deploy that fails");
    await waitFor(() => expect(discardOptimistic).toHaveBeenCalledWith("temp_99"));
    await waitFor(() => expect(composerInput().value).toBe("deploy that fails"));
    expect(screen.getByRole("link", { name: SIGN_IN_LABEL }).getAttribute("href")).toBe("/sign-in");
    // Once signed back in, Resend is the way out; a fresh 401 marks it again.
    expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy();
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(getPendingSends(SCOPE).map((entry) => entry.state)).toEqual([PENDING_SEND_STATE.SESSION]);
  });
});
