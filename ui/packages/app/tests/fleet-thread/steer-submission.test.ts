import { WS, ZID, appendMessage, capturedOnNew, capturedRetry, mockStream, renderThread, steerFleetActionMock } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import type { AppendMessage } from "@assistant-ui/react";
import { FleetThread } from "@/components/domain/FleetThread";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";

describe("FleetThread — steer submission", () => {
  it("ignores Retry when no delivery has failed", () => {
    mockStream([]);
    renderThread();
    expect(capturedRetry.current).toBeTypeOf("function");
    act(() => capturedRetry.current!());
    expect(steerFleetActionMock).not.toHaveBeenCalled();
  });

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
    const markOptimisticFailed = vi.fn();
    mockStream([], {
      appendOptimistic,
      reconcileOptimistic,
      markOptimisticFailed,
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
    expect(markOptimisticFailed).not.toHaveBeenCalled();
    expect(refreshed).toHaveBeenCalledTimes(1);
    unsubscribe();
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

  it("marks the optimistic message failed when the action returns ok:false", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_99");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], {
      appendOptimistic,
      reconcileOptimistic,
      markOptimisticFailed,
    });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
      errorCode: "UZ-AUTH-401",
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("deploy that fails"));
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_99"),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy that fails",
      "steer:pending",
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("retries a non-session send failure through the queue", async () => {
    const markOptimisticFailed = vi.fn();
    const appendOptimistic = vi.fn().mockReturnValue("temp_fail_1");
    const discardOptimistic = vi.fn();
    mockStream([], {
      appendOptimistic,
      markOptimisticFailed,
      discardOptimistic,
    });
    steerFleetActionMock
      .mockResolvedValueOnce({
        ok: false,
        error: "Provider unavailable",
        status: 503,
        errorCode: "UZ-AGT-503",
      })
      .mockResolvedValueOnce({ ok: true, data: { event_id: "evt_retry_ok" } });
    renderThread();
    await capturedOnNew.current!(appendMessage("retry this send"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(markOptimisticFailed).toHaveBeenCalledTimes(1);
    // The stale failed row leaves the thread before the fresh optimistic
    // re-submit — otherwise every retry stacks a duplicate of the message.
    expect(discardOptimistic).toHaveBeenCalledWith("temp_fail_1");
  });

  it("marks the optimistic message failed when the action invocation throws", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_t");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], {
      appendOptimistic,
      reconcileOptimistic,
      markOptimisticFailed,
    });
    steerFleetActionMock.mockRejectedValueOnce(
      new Error("Server Component transport failed"),
    );
    renderThread();
    await capturedOnNew.current!(appendMessage("offline send"));
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_t"),
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
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
