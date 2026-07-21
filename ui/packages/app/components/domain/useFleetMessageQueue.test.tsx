import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import type { AppendMessage } from "@assistant-ui/react";
import {
  __resetFleetMessageQueuesForTests,
  QUEUE_DELIVERY,
  useFleetDeliveryFailure,
  useFleetMessageQueue,
  type QueueDeliveryResult,
} from "./useFleetMessageQueue";

afterEach(() => {
  cleanup();
  __resetFleetMessageQueuesForTests();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function message(text: string): AppendMessage {
  return {
    role: "user",
    content: [{ type: "text", text }],
    createdAt: new Date(0),
    metadata: { custom: {} },
    parentId: null,
    sourceId: null,
    runConfig: undefined,
  };
}

function ServerQueueHarness({
  deliver,
}: {
  deliver: (value: AppendMessage) => Promise<QueueDeliveryResult>;
}) {
  useFleetDeliveryFailure("fleet_server");
  useFleetMessageQueue("fleet_server", false, deliver);
  return <span>server queue</span>;
}

describe("useFleetMessageQueue", () => {
  it("buffers messages while busy and drains on the falling edge", async () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const hook = renderHook(
      ({ busy }) => useFleetMessageQueue("fleet_1", busy, deliver),
      { initialProps: { busy: true } },
    );
    act(() => hook.result.current.queue.enqueue(message("queued"), { steer: false }));
    expect(deliver).not.toHaveBeenCalled();
    expect(hook.result.current.queue.items[0]?.prompt).toBe("queued");

    hook.rerender({ busy: false });
    await waitFor(() => expect(deliver).toHaveBeenCalledWith(message("queued")));
    expect(hook.result.current.queue.items).toHaveLength(0);
  });

  it("pauses after a failed delivery and retries it before later messages", async () => {
    const deliver = vi.fn()
      .mockResolvedValueOnce(QUEUE_DELIVERY.FAILED)
      .mockResolvedValueOnce(QUEUE_DELIVERY.WAITING);
    const hook = renderHook(() => useFleetMessageQueue("fleet_1", false, deliver));
    act(() => hook.result.current.queue.enqueue(message("first"), { steer: false }));
    await waitFor(() => expect(deliver).toHaveBeenCalledTimes(1));
    act(() => hook.result.current.queue.enqueue(message("second"), { steer: false }));
    expect(deliver).toHaveBeenCalledTimes(1);
    act(() => hook.result.current.retryMessage(message("first")));
    await waitFor(() => expect(deliver).toHaveBeenCalledTimes(2));
    expect(deliver.mock.calls[1]?.[0]).toEqual(message("first"));
  });

  it("removes a pending message before delivery", () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const hook = renderHook(() => useFleetMessageQueue("fleet_1", true, deliver));
    act(() => hook.result.current.queue.enqueue(message("remove me"), { steer: false }));
    const queuedId = hook.result.current.queue.items[0]!.id;
    act(() => hook.result.current.queue.remove(queuedId));
    expect(hook.result.current.queue.items).toHaveLength(0);
    expect(deliver).not.toHaveBeenCalled();
  });

  it("swallows a rejected delivery and leaves the driver paused", async () => {
    const deliver = vi.fn().mockRejectedValue(new Error("transport unavailable"));
    const hook = renderHook(() => useFleetMessageQueue("fleet_1", false, deliver));
    act(() => hook.result.current.queue.enqueue(message("fails"), { steer: false }));
    await waitFor(() => expect(deliver).toHaveBeenCalledTimes(1));
    expect(hook.result.current.queue.items).toHaveLength(0);
  });

  it("keeps a retried message queued while the fleet remains busy", async () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const hook = renderHook(
      ({ busy }) => useFleetMessageQueue("fleet_1", busy, deliver),
      { initialProps: { busy: true } },
    );
    act(() => hook.result.current.retryMessage(message("retry later")));
    expect(deliver).not.toHaveBeenCalled();
    hook.rerender({ busy: false });
    await waitFor(() => expect(deliver).toHaveBeenCalledWith(message("retry later")));
  });

  it("provides an empty server snapshot during server rendering", () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    vi.stubGlobal("window", undefined);
    expect(renderToStaticMarkup(<ServerQueueHarness deliver={deliver} />)).toContain(
      "server queue",
    );
    expect(deliver).not.toHaveBeenCalled();
  });

  it("preserves pending messages across Chat unmount and remount", async () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const first = renderHook(() =>
      useFleetMessageQueue("fleet_persistent", true, deliver),
    );
    act(() => first.result.current.queue.enqueue(message("survive navigation"), { steer: false }));
    first.unmount();

    renderHook(() => useFleetMessageQueue("fleet_persistent", false, deliver));
    await waitFor(() =>
      expect(deliver).toHaveBeenCalledWith(message("survive navigation")),
    );
  });

  it("immediately advances after delivery already completed on the live stream", async () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.COMPLETE);
    const hook = renderHook(
      ({ busy }) => useFleetMessageQueue("fleet_fast", busy, deliver),
      { initialProps: { busy: true } },
    );
    act(() => {
      hook.result.current.queue.enqueue(message("first"), { steer: false });
      hook.result.current.queue.enqueue(message("second"), { steer: false });
    });
    hook.rerender({ busy: false });
    await waitFor(() => expect(deliver).toHaveBeenCalledTimes(2));
  });

  it("preserves a failed delivery across Chat unmount and remount", () => {
    const first = renderHook(() => useFleetDeliveryFailure("fleet_failed"));
    act(() => {
      first.result.current.setFailedDelivery({
        message: message("retry after navigation"),
        kind: "send",
      });
    });
    first.unmount();

    const second = renderHook(() => useFleetDeliveryFailure("fleet_failed"));
    expect(second.result.current.failedDelivery?.message).toEqual(
      message("retry after navigation"),
    );
    act(() => second.result.current.clearFailedDelivery());
    expect(second.result.current.failedDelivery).toBeNull();
  });

  it("keeps a shared queue alive until its final Chat consumer unmounts", () => {
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const first = renderHook(() =>
      useFleetMessageQueue("fleet_shared", true, deliver),
    );
    const second = renderHook(() =>
      useFleetMessageQueue("fleet_shared", true, deliver),
    );
    expect(first.result.current.queue).toBe(second.result.current.queue);
    first.unmount();
    act(() => {
      second.result.current.queue.enqueue(message("still retained"), { steer: false });
    });
    expect(second.result.current.queue.items).toHaveLength(1);
  });

  it("evicts an idle queue after its release window", () => {
    vi.useFakeTimers();
    const deliver = vi.fn().mockResolvedValue(QUEUE_DELIVERY.WAITING);
    const first = renderHook(() =>
      useFleetMessageQueue("fleet_idle", false, deliver),
    );
    const firstAdapter = first.result.current.queue;
    first.unmount();
    act(() => {
      vi.advanceTimersByTime(30_000);
    });

    const second = renderHook(() =>
      useFleetMessageQueue("fleet_idle", false, deliver),
    );
    expect(second.result.current.queue).not.toBe(firstAdapter);
  });

  it("supports multiple failure subscribers and a writer after unmount", () => {
    const first = renderHook(() => useFleetDeliveryFailure("fleet_failure_shared"));
    const second = renderHook(() => useFleetDeliveryFailure("fleet_failure_shared"));
    const writeAfterUnmount = first.result.current.setFailedDelivery;
    first.unmount();
    act(() => {
      second.result.current.setFailedDelivery({
        message: message("one listener remains"),
        kind: "session",
      });
    });
    expect(second.result.current.failedDelivery?.kind).toBe("session");
    second.unmount();
    act(() => {
      writeAfterUnmount({ message: message("no listeners"), kind: "send" });
    });
  });

  it("notifies mounted failure consumers when the test registry resets", () => {
    const hook = renderHook(() => useFleetDeliveryFailure("fleet_reset"));
    act(() => {
      hook.result.current.setFailedDelivery({
        message: message("clear me"),
        kind: "send",
      });
    });
    expect(hook.result.current.failedDelivery).not.toBeNull();
    act(() => __resetFleetMessageQueuesForTests());
    expect(hook.result.current.failedDelivery).toBeNull();
  });
});
