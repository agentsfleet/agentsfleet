import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import type { AppendMessage } from "@assistant-ui/react";
import {
  useFleetDeliveryFailure,
  __resetFleetDeliveryFailuresForTests,
} from "./useFleetDeliveryFailure";

function message(text: string): AppendMessage {
  return {
    role: "user",
    content: [{ type: "text", text }],
    attachments: [],
    metadata: { custom: {} },
    parentId: null,
    sourceId: null,
    runConfig: {},
    startRun: true,
  } as unknown as AppendMessage;
}

afterEach(() => {
  cleanup();
  __resetFleetDeliveryFailuresForTests();
  vi.clearAllMocks();
});

describe("useFleetDeliveryFailure", () => {
  it("starts with nothing failed", () => {
    const hook = renderHook(() => useFleetDeliveryFailure("fleet_clean"));
    expect(hook.result.current.failedDelivery).toBeNull();
  });

  it("preserves a failed delivery across Chat unmount and remount", () => {
    const first = renderHook(() => useFleetDeliveryFailure("fleet_failed"));
    act(() => {
      first.result.current.setFailedDelivery({
        message: message("retry after navigation"),
        tempId: "optim-1",
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

  it("keeps one fleet's failure out of another fleet's composer", () => {
    const mine = renderHook(() => useFleetDeliveryFailure("fleet_a"));
    const other = renderHook(() => useFleetDeliveryFailure("fleet_b"));
    act(() => {
      mine.result.current.setFailedDelivery({ message: message("mine"), tempId: "optim-1", kind: "send" });
    });
    expect(mine.result.current.failedDelivery).not.toBeNull();
    expect(other.result.current.failedDelivery).toBeNull();
  });

  it("supports multiple failure subscribers and a writer after unmount", () => {
    const first = renderHook(() => useFleetDeliveryFailure("fleet_failure_shared"));
    const second = renderHook(() => useFleetDeliveryFailure("fleet_failure_shared"));
    const writeAfterUnmount = first.result.current.setFailedDelivery;
    first.unmount();
    act(() => {
      second.result.current.setFailedDelivery({
        message: message("one listener remains"),
        tempId: "optim-2",
        kind: "session",
      });
    });
    expect(second.result.current.failedDelivery?.kind).toBe("session");
    second.unmount();
    act(() => {
      writeAfterUnmount({ message: message("no listeners"), tempId: "optim-3", kind: "send" });
    });
  });

  it("reads as nothing on the server, where no browser registry exists", () => {
    // The registry is module state in the browser. Server-rendering must not
    // read it — a failure from one request would leak into another's markup.
    function Probe() {
      const { failedDelivery } = useFleetDeliveryFailure("fleet_ssr");
      return <span>{failedDelivery === null ? "no failure" : "leaked"}</span>;
    }
    expect(renderToStaticMarkup(<Probe />)).toContain("no failure");
  });

  it("notifies mounted failure consumers when the test registry resets", () => {
    const hook = renderHook(() => useFleetDeliveryFailure("fleet_reset"));
    act(() => {
      hook.result.current.setFailedDelivery({
        message: message("clear me"),
        tempId: "optim-4",
        kind: "send",
      });
    });
    expect(hook.result.current.failedDelivery).not.toBeNull();
    act(() => __resetFleetDeliveryFailuresForTests());
    expect(hook.result.current.failedDelivery).toBeNull();
  });
});
