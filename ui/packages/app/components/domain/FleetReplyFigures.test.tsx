import { ev, mockStream, renderThread } from "@/tests/fleet-thread/harness";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen } from "@testing-library/react";

import { REPLY_FIGURES_TEST_ID, WORKED_FOR } from "./FleetReplyFigures";
import type { FleetEvent, FleetEventStatus } from "@/lib/streaming/fleet-stream-row";

// What a turn took, at its foot once it settles, and how long it has run while
// it has nothing else to show. Rendered through the thread, from the figures
// the reply's own custom bag carries.

const NOW = Date.UTC(2026, 9, 4, 12, 0, 0);
const TOKENS = 12_400;
const WALL_MS = 41_000;
const COST_NANOS = 30_000_000;
const RUNNING_FOR_MS = 65_000;
const TICK_MS = 1_000;
const PROCESSED: FleetEventStatus = "processed";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
});

function renderTurn(over: Partial<FleetEvent>) {
  mockStream([ev({ id: "evt_turn", role: "user", actor: "operator", text: "Go", reply: "Done.", status: PROCESSED, ...over })]);
  renderThread();
}

const figures = () => screen.queryByTestId(REPLY_FIGURES_TEST_ID)?.textContent ?? null;

describe("FleetThread — reply figures", () => {
  it("test_settled_reply_shows_figures", () => {
    renderTurn({ tokens: TOKENS, wallMs: WALL_MS, costNanos: COST_NANOS });
    // Pin test: the literal is the line Codex ends a turn with, plus the spend.
    expect(figures()).toBe(`${WORKED_FOR} 41s · 12.4K tokens · $0.03`);
  });

  it("test_unknown_figure_left_out", () => {
    renderTurn({ tokens: TOKENS, wallMs: WALL_MS, costNanos: null });
    expect(figures()).toBe(`${WORKED_FOR} 41s · 12.4K tokens`);
    expect(figures()).not.toContain("$");
    cleanup();
    renderTurn({ tokens: TOKENS, wallMs: null, costNanos: null });
    expect(figures()).toBe("12.4K tokens");
    cleanup();
    // Nothing reported, no line at all.
    renderTurn({ tokens: null, wallMs: null, costNanos: null });
    expect(figures()).toBeNull();
  });

  it("test_running_reply_shows_elapsed", () => {
    renderTurn({ status: "received", reply: "", createdAt: new Date(NOW - RUNNING_FOR_MS), tokens: TOKENS });
    const working = screen.getByRole("status", { name: "Working" });
    const clock = () => working.querySelector("[data-waiting-elapsed]")?.textContent;
    expect(clock()).toBe("(1m 5s)");
    // The clock is for the eye: the status keeps its name and its spoken text.
    expect(working.querySelector("[data-waiting-elapsed]")?.getAttribute("aria-hidden")).toBe("true");
    act(() => {
      vi.advanceTimersByTime(TICK_MS);
    });
    expect(clock()).toBe("(1m 6s)");
    // A running reply draws no figures line, even with a figure in hand.
    expect(figures()).toBeNull();
  });
});
