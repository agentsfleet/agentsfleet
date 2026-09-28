import { ev, mockStream, renderThread, threadElement } from "./fleet-thread/harness";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, within } from "@testing-library/react";

// Tool rows as assistant-ui tool-call parts: the library's `useToolCallElapsed`
// reads each part's timing and ticks only while the part runs.

const NOW = 10_000;
const RUNNING_FOR_MS = 2_000;
const TICK_MS = 1_000;
const DONE_MS = 700;
const TOOL_CALLS = "Tool calls";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("FleetThread — tool rows", () => {
  it("test_tool_row_reads_part_timing", () => {
    const working = ev({
      id: "evt_tools", role: "user", actor: "operator", text: "Look it up", status: "received",
      tools: [
        { name: "read_file", startedAtMs: NOW - 5_000, ms: DONE_MS, done: true },
        { name: "search_repo", startedAtMs: NOW - RUNNING_FOR_MS, ms: null, done: false },
      ],
    });
    mockStream([working]);
    const view = renderThread();
    // Adjacent calls share one list.
    const lists = screen.getAllByRole("list", { name: TOOL_CALLS });
    expect(lists).toHaveLength(1);
    const running = within(lists[0]!).getByText("search_repo").closest("li")!;
    const done = within(lists[0]!).getByText("read_file").closest("li")!;
    // Pin test: the literals are the rendered clocks.
    expect(done.getAttribute("data-done")).toBe("true");
    expect(done.textContent).toContain("✓");
    expect(done.textContent).toContain("0.7s");
    expect(running.getAttribute("data-done")).toBeNull();
    expect(running.textContent).toContain("2.0s");
    act(() => {
      vi.advanceTimersByTime(TICK_MS);
    });
    expect(running.textContent).toContain("3.0s");
    // The ticking clock is hidden from assistive tech; a finished one is read.
    const clockOf = (row: Element) => row.querySelector(".tabular-nums")!;
    expect(clockOf(running).getAttribute("aria-hidden")).toBe("true");
    expect(clockOf(done).getAttribute("aria-hidden")).toBeNull();

    // The turn settles with the call never reported done: no clock claims it runs.
    mockStream([{ ...working, status: "processed", reply: "Found it." }]);
    view.rerender(threadElement());
    const stranded = screen.getByText("search_repo").closest("li")!;
    expect(stranded.textContent).not.toMatch(/\d\.\ds/);
  });
});
