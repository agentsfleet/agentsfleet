import { ev, mockStream, renderThread } from "@/tests/fleet-thread/harness";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";

import { REPLY_FIGURES_TEST_ID, RUN_DETAILS_LABEL } from "./FleetReplyFigures";
import type { FleetEvent, FleetEventStatus } from "@/lib/streaming/fleet-stream-row";

// Optional reported figures, and the waiting clock, through the real thread.

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
  it("opens reported figures on request and closes with Escape", () => {
    renderTurn({ tokens: TOKENS, wallMs: WALL_MS, costNanos: String(COST_NANOS) });
    const trigger = screen.getByRole("button", { name: RUN_DETAILS_LABEL });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: RUN_DETAILS_LABEL });
    for (const value of ["Duration", "41s", "Tokens", "12.4K", "Reported cost", "$0.03"]) {
      expect(within(dialog).getByText(value)).toBeTruthy();
    }
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("test_unknown_figure_left_out", () => {
    renderTurn({ tokens: TOKENS, wallMs: WALL_MS, costNanos: null });
    fireEvent.click(screen.getByRole("button", { name: RUN_DETAILS_LABEL }));
    expect(screen.getByRole("dialog").textContent).not.toContain("Reported cost");
    cleanup();
    renderTurn({ tokens: TOKENS, wallMs: null, costNanos: null });
    fireEvent.click(screen.getByRole("button", { name: RUN_DETAILS_LABEL }));
    expect(screen.getByRole("dialog").textContent).toContain("12.4K");
    expect(screen.getByRole("dialog").textContent).not.toContain("Duration");
    cleanup();
    // Nothing reported, no line at all.
    renderTurn({ tokens: null, wallMs: null, costNanos: null });
    expect(figures()).toBeNull();
  });

  it("should draw no figures line while the final reply is still being read", () => {
    renderTurn({ reply: "", replyRecovering: true, tokens: TOKENS, wallMs: WALL_MS, costNanos: String(COST_NANOS) });
    expect(screen.getByText(/Loading final reply/)).toBeTruthy();
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
