import { WS, ZID, appendMessage, capturedOnNew, capturedSubmittedMessageId, ev, mockStream, renderThread, steerFleetActionMock, threadElement } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import type { AppendMessage } from "@assistant-ui/react";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";

const COMPOSER_NAME = "Message this fleet…";
const SEND_FAILED_TEXT = "Message not sent.";
const RESEND_LABEL = "Resend";
const RESTORE_LABEL = "Restore";
const SIGN_IN_LABEL = "Sign in";
// Longer than any retry schedule the action could run: a replay would land
// inside it.
const NO_REPLAY_WINDOW_MS = 5_000;
const UNAVAILABLE = { ok: false, error: "Provider unavailable", status: 503, errorCode: "UZ-AGT-503" } as const;

function composerInput(): HTMLTextAreaElement {
  return screen.getByRole("textbox", { name: COMPOSER_NAME }) as HTMLTextAreaElement;
}

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
            resolve({ ok: true, data: { event_id: "evt_1" } });
          };
        }),
    );
    steerFleetActionMock.mockImplementationOnce(
      (_ws: string, _z: string, text: string) => {
        order.push(text);
        return Promise.resolve({ ok: true, data: { event_id: "evt_2" } });
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
  });

  it("calls steerFleetAction and reconciles the optimistic message on ok", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_42");
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic,
      reconcileOptimistic,
      discardOptimistic,
    });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_real_42" },
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("deploy the canary"));
    await waitFor(() =>
      expect(steerFleetActionMock).toHaveBeenCalledWith(
        WS,
        ZID,
        "deploy the canary",
      ),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy the canary",
      "steer:pending",
    );
    expect(reconcileOptimistic).toHaveBeenCalledWith("temp_42", "evt_real_42");
    expect(discardOptimistic).not.toHaveBeenCalled();
    expect(refreshed).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it("keeps submit scroll intent through acknowledgement and reordered backfill", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_clock_skew");
    mockStream([], { appendOptimistic });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_clock_skew" },
    });
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
    steerFleetActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_already_complete" },
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("fast completion"));
    expect(reconcileOptimistic).toHaveBeenCalledWith(
      "temp_1",
      "evt_already_complete",
    );
  });

  it("test_failed_send_leaves_thread_and_restores_draft", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic: vi.fn().mockReturnValue("temp_t"),
      reconcileOptimistic,
      discardOptimistic,
    });
    steerFleetActionMock.mockRejectedValueOnce(new Error("Server Component transport failed"));
    renderThread();
    await act(async () => {
      await capturedOnNew.current!(appendMessage("offline send"));
    });
    expect(discardOptimistic).toHaveBeenCalledWith("temp_t");
    await waitFor(() => expect(composerInput().value).toBe("offline send"));
    expect(screen.getByText(SEND_FAILED_TEXT)).toBeTruthy();
    expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy();
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("test_resend_submits_restored_text_once", async () => {
    const discardOptimistic = vi.fn();
    const reconcileOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic: vi.fn().mockReturnValueOnce("temp_fail_1").mockReturnValueOnce("temp_resend"),
      discardOptimistic,
      reconcileOptimistic,
    });
    steerFleetActionMock
      .mockResolvedValueOnce(UNAVAILABLE)
      .mockResolvedValueOnce({ ok: true, data: { event_id: "evt_resend_ok" } });
    // A refused send stays refused until the operator acts. The clock is fake
    // from before the send, so a replay timer armed by the refusal would fire
    // inside the window below.
    vi.useFakeTimers();
    try {
      renderThread();
      await act(async () => {
        await capturedOnNew.current!(appendMessage("retry this send"));
      });
      expect(composerInput().value).toBe("retry this send");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(NO_REPLAY_WINDOW_MS);
      });
      expect(steerFleetActionMock).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }

    fireEvent.click(screen.getByRole("button", { name: RESEND_LABEL }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(steerFleetActionMock).toHaveBeenLastCalledWith(WS, ZID, "retry this send");
    await waitFor(() => expect(reconcileOptimistic).toHaveBeenCalledWith("temp_resend", "evt_resend_ok"));
    expect(screen.queryByText(SEND_FAILED_TEXT)).toBeNull();
    expect(discardOptimistic).toHaveBeenCalledTimes(1);
  });

  it("test_failure_restore_respects_existing_draft", async () => {
    let refuse: () => void = () => {};
    steerFleetActionMock.mockImplementationOnce(
      () => new Promise((resolve) => { refuse = () => resolve(UNAVAILABLE); }),
    );
    mockStream([], { appendOptimistic: vi.fn().mockReturnValue("temp_old") });
    const view = renderThread();
    let sent: Promise<void> = Promise.resolve();
    act(() => {
      sent = capturedOnNew.current!(appendMessage("old"));
    });
    // The POST is chained behind the delivery tail; refuse it once it is out.
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(1));
    fireEvent.change(composerInput(), { target: { value: "new" } });
    await act(async () => {
      refuse();
      await sent;
    });
    // The operator's newer draft is never overwritten by the refused text.
    expect(composerInput().value).toBe("new");
    expect(screen.queryByRole("button", { name: RESEND_LABEL })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: RESTORE_LABEL }));
    expect(composerInput().value).toBe("old\n\nnew");
    expect(screen.getByRole("button", { name: RESEND_LABEL })).toBeTruthy();

    // A remount starts from an empty composer and gets the refused text back.
    view.unmount();
    renderThread();
    await waitFor(() => expect(composerInput().value).toBe("old"));
  });

  it("test_session_failure_keeps_sign_in", async () => {
    const reconcileOptimistic = vi.fn();
    const discardOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic: vi.fn().mockReturnValue("temp_99"),
      reconcileOptimistic,
      discardOptimistic,
    });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
      errorCode: "UZ-AUTH-401",
    });
    renderThread();
    await act(async () => {
      await capturedOnNew.current!(appendMessage("deploy that fails"));
    });
    expect(discardOptimistic).toHaveBeenCalledWith("temp_99");
    await waitFor(() => expect(composerInput().value).toBe("deploy that fails"));
    expect(screen.getByRole("link", { name: SIGN_IN_LABEL }).getAttribute("href")).toBe("/sign-in");
    expect(screen.queryByRole("button", { name: RESEND_LABEL })).toBeNull();
    expect(reconcileOptimistic).not.toHaveBeenCalled();
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
