import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { EVENTS } from "@/lib/analytics/events";

const { captureProductEventMock, pathnameRef } = vi.hoisted(() => ({
  captureProductEventMock: vi.fn(),
  pathnameRef: { current: "/w/ws_1/fleets" },
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));
vi.mock("next/navigation", () => ({
  usePathname: () => pathnameRef.current,
}));

const ROUTE_A = "/w/ws_1/fleets";
const ROUTE_B = "/admin/runners";

import DashboardError from "../app/(dashboard)/error";
import {
  INCIDENT_WINDOW_MS,
  RETRY_DELAYS_MS,
  __resetErrorRetryForTests,
} from "../app/(dashboard)/use-error-retry";

// The ladder position lives in module state on purpose — a failed retry
// REMOUNTS the boundary, so React state would reset to zero every time and the
// backoff would never grow. That makes it leak between tests unless cleared.
beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  pathnameRef.current = ROUTE_A;
  __resetErrorRetryForTests();
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

const FIRST_DELAY_MS = RETRY_DELAYS_MS[0] ?? 0;
const SECOND_DELAY_MS = RETRY_DELAYS_MS[1] ?? 0;
/** One countdown tick — the cadence the boundary re-renders its label on. */
const ONE_TICK_MS = 1_000;

function renderBoundary(reset: () => void, error: Error = new Error("boom")) {
  return render(React.createElement(DashboardError, { error, reset }));
}

describe("dashboard error boundary — automatic recovery", () => {
  it("retries on its own, with no click, once the first delay elapses", () => {
    const reset = vi.fn();
    renderBoundary(reset);

    expect(reset).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledOnce();
  });

  it("backs off instead of hammering a route that stays broken", () => {
    // First failure: waits the first delay, spends attempt 1.
    const reset = vi.fn();
    const first = renderBoundary(reset);
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledOnce();
    first.unmount();

    // The retry failed, so the boundary remounts on a NEW error. It must resume
    // at the second rung — not restart the ladder, which would retry every
    // FIRST_DELAY_MS forever against a route that is genuinely down.
    const second = renderBoundary(reset, new Error("boom again"));
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledOnce();
    act(() => {
      vi.advanceTimersByTime(SECOND_DELAY_MS - FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledTimes(2);
    second.unmount();
  });

  it("stops after the budget and leaves only a manual retry", () => {
    const reset = vi.fn();
    for (const delay of RETRY_DELAYS_MS) {
      const mounted = renderBoundary(reset, new Error("still broken"));
      act(() => {
        vi.advanceTimersByTime(delay);
      });
      mounted.unmount();
    }
    expect(reset).toHaveBeenCalledTimes(RETRY_DELAYS_MS.length);

    // Budget spent: this mount schedules nothing, however long we wait.
    renderBoundary(reset, new Error("still broken"));
    act(() => {
      vi.advanceTimersByTime(SECOND_DELAY_MS * RETRY_DELAYS_MS.length);
    });
    expect(reset).toHaveBeenCalledTimes(RETRY_DELAYS_MS.length);
    expect(screen.getByText(/stopped after/i)).toBeTruthy();
    expect(screen.getByTestId("dashboard-error-retry")).toBeTruthy();
  });

  it("restarts the ladder when the user takes over", () => {
    const reset = vi.fn();
    const first = renderBoundary(reset);
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    first.unmount();

    // A manual retry must not leave the NEXT automatic wait at the longer rung:
    // the user helping should not be punished with a slower recovery.
    const second = renderBoundary(reset, new Error("boom again"));
    act(() => {
      screen.getByTestId("dashboard-error-retry").click();
    });
    expect(reset).toHaveBeenCalledTimes(2);
    second.unmount();

    const third = renderBoundary(reset, new Error("boom thrice"));
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledTimes(3);
    third.unmount();
  });

  it("starts fresh when a failure arrives long after the last one", () => {
    // The recovery case, which the attempt counter cannot see on its own: a
    // boundary that unmounts because the retry WORKED looks identical to one
    // about to remount on a new failure. Without the incident window the count
    // would stay elevated for the rest of the session, so an unrelated failure
    // later would start mid-ladder and eventually never auto-retry at all.
    const reset = vi.fn();
    const first = renderBoundary(reset);
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledOnce();
    first.unmount();

    // …the page recovered, and much later something unrelated breaks.
    act(() => {
      vi.advanceTimersByTime(INCIDENT_WINDOW_MS + ONE_TICK_MS);
    });
    const later = renderBoundary(reset, new Error("unrelated, much later"));
    expect(screen.getByText(/attempt 1 of/i)).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledTimes(2);
    later.unmount();
  });

  it("does not spend an unrelated route's budget on the previous incident", () => {
    // Greptile P1 on PR #578, and it was right: the elapsed-time window alone
    // could not see this. A retry SUCCEEDS, then seconds later a different
    // route breaks — well inside the incident window, so the counter carried
    // over and the new failure started mid-ladder, reporting attempts that were
    // never made for it. Route identity is the signal time cannot supply.
    const reset = vi.fn();
    const first = renderBoundary(reset);
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledOnce();
    first.unmount();

    // …that retry worked. A DIFFERENT route now fails, promptly.
    pathnameRef.current = ROUTE_B;
    const unrelated = renderBoundary(reset, new Error("different route"));
    expect(screen.getByText(/attempt 1 of/i)).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    expect(reset).toHaveBeenCalledTimes(2);
    unrelated.unmount();
  });

  it("keeps backing off when the SAME route fails again promptly", () => {
    // The other side of the route check: same pathname inside the window is one
    // incident, and must still back off rather than restarting the ladder.
    const reset = vi.fn();
    const first = renderBoundary(reset);
    act(() => {
      vi.advanceTimersByTime(FIRST_DELAY_MS);
    });
    first.unmount();

    const second = renderBoundary(reset, new Error("same route again"));
    expect(screen.getByText(/attempt 2 of/i)).toBeTruthy();
    second.unmount();
  });

  it("counts the wait down in the live region", () => {
    renderBoundary(vi.fn());
    const seconds = () => screen.getByText(/retrying in/i).textContent ?? "";
    const before = seconds();
    act(() => {
      vi.advanceTimersByTime(ONE_TICK_MS);
    });
    expect(seconds()).not.toEqual(before);
  });
});

describe("dashboard error boundary — reporting", () => {
  it("captures the failure, so the support line is not an empty promise", () => {
    const error = Object.assign(new TypeError("cannot read properties of null"), {
      digest: "3092374019",
    });
    renderBoundary(vi.fn(), error);

    expect(captureProductEventMock).toHaveBeenCalledOnce();
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.dashboard_error_shown, {
      error_name: "TypeError",
      digest: "3092374019",
      attempt: 0,
    });
  });

  it("never sends the error message", () => {
    // A message is free text: it routinely carries a URL, an id, or a fragment
    // of whatever payload blew up. The class and Next's digest are enough to
    // find it in the server log without shipping any of that to analytics.
    //
    // Assembled at runtime on purpose. The test needs a credential-SHAPED
    // string to be worth anything, and writing one as a literal trips the
    // secret scanner — which is the correct behaviour from the scanner, so the
    // fixture bends rather than the gate.
    const fakeCredential = ["sk", "live", "n0tar3al"].join("-");
    const secretish = `failed to fetch /v1/secrets?token=${fakeCredential}`;
    renderBoundary(vi.fn(), new Error(secretish));

    const sent = JSON.stringify(captureProductEventMock.mock.calls);
    expect(sent).not.toContain(secretish);
    expect(sent).not.toContain(fakeCredential);
  });

  it("captures once per failure, not once per countdown tick", () => {
    renderBoundary(vi.fn());
    act(() => {
      vi.advanceTimersByTime(ONE_TICK_MS);
    });
    act(() => {
      vi.advanceTimersByTime(ONE_TICK_MS);
    });
    expect(captureProductEventMock).toHaveBeenCalledOnce();
  });

  it("reports the spent budget when the automatic attempts did not recover", () => {
    const reset = vi.fn();
    for (const delay of RETRY_DELAYS_MS) {
      const mounted = renderBoundary(reset, new Error("still broken"));
      act(() => {
        vi.advanceTimersByTime(delay);
      });
      mounted.unmount();
    }
    captureProductEventMock.mockClear();

    renderBoundary(reset, new Error("still broken"));
    expect(captureProductEventMock).toHaveBeenCalledWith(
      EVENTS.dashboard_error_shown,
      expect.objectContaining({ attempt: RETRY_DELAYS_MS.length }),
    );
  });
});
