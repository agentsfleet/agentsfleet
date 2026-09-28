import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { __resetPendingSendsForTests } from "@/lib/streaming/pending-sends";
import { PENDING_SEND_STATE, useFleetPendingSends } from "./useFleetPendingSends";

const WS = "ws_hook";
const SUBMITTED_AT_MS = 1_700_000_000_000;

afterEach(() => {
  cleanup();
  __resetPendingSendsForTests();
});

describe("useFleetPendingSends", () => {
  it("starts with nothing pending", () => {
    const hook = renderHook(() => useFleetPendingSends(WS, "fleet_clean"));
    expect(hook.result.current.pending).toEqual([]);
  });

  it("preserves an unresolved send across Chat unmount and remount", () => {
    const first = renderHook(() => useFleetPendingSends(WS, "fleet_failed"));
    act(() => {
      first.result.current.begin({ operationId: "op-1", text: "retry after navigation", submittedAtMs: SUBMITTED_AT_MS });
      first.result.current.fail("op-1", PENDING_SEND_STATE.REFUSED);
    });
    first.unmount();

    const second = renderHook(() => useFleetPendingSends(WS, "fleet_failed"));
    expect(second.result.current.pending).toEqual([
      { operationId: "op-1", text: "retry after navigation", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: SUBMITTED_AT_MS },
    ]);
    expect(second.result.current.byText("retry after navigation")?.operationId).toBe("op-1");
    act(() => second.result.current.dismiss("op-1"));
    expect(second.result.current.pending).toEqual([]);
  });

  it("keeps one fleet's sends out of another fleet's composer", () => {
    const mine = renderHook(() => useFleetPendingSends(WS, "fleet_a"));
    const other = renderHook(() => useFleetPendingSends(WS, "fleet_b"));
    act(() => {
      mine.result.current.begin({ operationId: "op-mine", text: "mine", submittedAtMs: SUBMITTED_AT_MS });
    });
    expect(mine.result.current.pending).toHaveLength(1);
    expect(other.result.current.pending).toEqual([]);
  });

  it("supports several subscribers and a writer after unmount", () => {
    const first = renderHook(() => useFleetPendingSends(WS, "fleet_shared"));
    const second = renderHook(() => useFleetPendingSends(WS, "fleet_shared"));
    const writeAfterUnmount = first.result.current.begin;
    first.unmount();
    act(() => {
      second.result.current.begin({ operationId: "op-2", text: "one listener remains", submittedAtMs: SUBMITTED_AT_MS });
    });
    expect(second.result.current.pending.map((entry) => entry.operationId)).toEqual(["op-2"]);
    second.unmount();
    act(() => {
      writeAfterUnmount({ operationId: "op-3", text: "no listeners", submittedAtMs: SUBMITTED_AT_MS });
    });
  });

  it("test_ledger_reads_empty_on_server", () => {
    // The ledger is browser state. Server-rendering must not read it — one
    // request's unresolved sends would leak into another's markup.
    function Probe() {
      const { pending } = useFleetPendingSends(WS, "fleet_ssr");
      return <span>{pending.length === 0 ? "no pending" : "leaked"}</span>;
    }
    expect(renderToStaticMarkup(<Probe />)).toContain("no pending");
  });

  it("notifies mounted consumers when the test ledger resets", () => {
    const hook = renderHook(() => useFleetPendingSends(WS, "fleet_reset"));
    act(() => {
      hook.result.current.begin({ operationId: "op-r", text: "clear me", submittedAtMs: SUBMITTED_AT_MS });
    });
    expect(hook.result.current.pending).toHaveLength(1);
    act(() => __resetPendingSendsForTests());
    expect(hook.result.current.pending).toEqual([]);
  });
});
