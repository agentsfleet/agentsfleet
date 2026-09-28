import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { __resetPendingSendsForTests, beginPendingSend } from "@/lib/streaming/pending-sends";
import { PENDING_SEND_STATE, useFleetPendingSends, type LedgerScope } from "./useFleetPendingSends";

const SUBMITTED_AT_MS = 1_700_000_000_000;

function scope(fleetId: string): LedgerScope {
  return { subject: "user_hook", workspaceId: "ws_hook", fleetId };
}

afterEach(() => {
  cleanup();
  __resetPendingSendsForTests();
});

describe("useFleetPendingSends", () => {
  it("starts with nothing pending", () => {
    const hook = renderHook(() => useFleetPendingSends(scope("fleet_clean")));
    expect(hook.result.current.pending).toEqual([]);
  });

  it("preserves an unresolved send across Chat unmount and remount", () => {
    const first = renderHook(() => useFleetPendingSends(scope("fleet_failed")));
    act(() => {
      first.result.current.writers.begin({ operationId: "op-1", text: "retry after navigation", submittedAtMs: SUBMITTED_AT_MS });
      first.result.current.writers.fail("op-1", PENDING_SEND_STATE.REFUSED);
    });
    first.unmount();

    const second = renderHook(() => useFleetPendingSends(scope("fleet_failed")));
    expect(second.result.current.pending).toEqual([
      { operationId: "op-1", text: "retry after navigation", state: PENDING_SEND_STATE.REFUSED, submittedAtMs: SUBMITTED_AT_MS },
    ]);
    expect(second.result.current.writers.find("op-1")?.text).toBe("retry after navigation");
    act(() => second.result.current.writers.dismiss("op-1"));
    expect(second.result.current.pending).toEqual([]);
  });

  it("keeps its writers stable across ledger writes, so a callback built on them is not rebuilt", () => {
    const hook = renderHook(() => useFleetPendingSends(scope("fleet_stable")));
    const writers = hook.result.current.writers;
    act(() => {
      writers.begin({ operationId: "op-s", text: "stable", submittedAtMs: SUBMITTED_AT_MS });
      writers.settle("op-s");
    });
    expect(hook.result.current.writers).toBe(writers);
  });

  it("keeps one fleet's sends out of another fleet's composer", () => {
    const mine = renderHook(() => useFleetPendingSends(scope("fleet_a")));
    const other = renderHook(() => useFleetPendingSends(scope("fleet_b")));
    act(() => {
      mine.result.current.writers.begin({ operationId: "op-mine", text: "mine", submittedAtMs: SUBMITTED_AT_MS });
    });
    expect(mine.result.current.pending).toHaveLength(1);
    expect(other.result.current.pending).toEqual([]);
  });

  it("test_ledger_reads_empty_on_server", () => {
    // The ledger is browser state. Server-rendering must not read it — one
    // request's unresolved sends would leak into another's markup. The ledger
    // holds an entry, so a leak is what the markup would show.
    beginPendingSend(scope("fleet_ssr"), { operationId: "op-ssr", text: "typed in a browser", submittedAtMs: SUBMITTED_AT_MS });
    function Probe() {
      const { pending } = useFleetPendingSends(scope("fleet_ssr"));
      return <span>{pending.length === 0 ? "no pending" : "leaked"}</span>;
    }
    expect(renderToStaticMarkup(<Probe />)).toContain("no pending");
  });

  it("notifies mounted consumers when the test ledger resets", () => {
    const hook = renderHook(() => useFleetPendingSends(scope("fleet_reset")));
    act(() => {
      hook.result.current.writers.begin({ operationId: "op-r", text: "clear me", submittedAtMs: SUBMITTED_AT_MS });
    });
    expect(hook.result.current.pending).toHaveLength(1);
    act(() => __resetPendingSendsForTests());
    expect(hook.result.current.pending).toEqual([]);
  });
});
