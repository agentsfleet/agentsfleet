import { FLEET_NAME, ev, mockStream, renderThread, threadElement } from "./harness";
import { describe, expect, it } from "vitest";
import { act, render, screen } from "@testing-library/react";
import { OUTCOME } from "@/lib/events/event-summary";
import { FleetThread } from "@/components/domain/FleetThread";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";

describe("FleetThread — role rendering: turns and connection", () => {
  it("renders an operator steer and the fleet's reply as separate conversation turns", () => {
    // A durable event can contain both the trigger and its response. The
    // transcript normalizes those into two assistant-ui message roots so
    // alignment, scrolling, and future actions remain message-scoped.
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "please review PR 517",
        reply:
          "Reviewed. Two suggestions, Continuous Integration (CI) passing.",
        status: "processed",
      }),
    ]);
    const { container } = renderThread();
    // The operator's question survives (the old code dropped it for the reply).
    expect(screen.getByText(/please review PR 517/)).toBeTruthy();
    // The fleet's reply survives too without repeating sender chrome.
    expect(screen.getByText(/Reviewed\. Two suggestions/)).toBeTruthy();
    expect(
      screen.getByText("Operator:", { selector: ".sr-only" }),
    ).toBeTruthy();
    expect(
      screen.getByText(`${FLEET_NAME}:`, { selector: ".sr-only" }),
    ).toBeTruthy();
    expect(container.querySelectorAll("[data-message-id]")).toHaveLength(2);
    const replyRow = screen
      .getByText(/Reviewed\. Two suggestions/)
      .closest("[data-role]");
    expect(replyRow?.getAttribute("data-role")).toBe("assistant");
  });

  it("announces an API steer with its real sender", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:api",
        text: "review from automation",
      }),
    ]);
    renderThread();

    expect(screen.getByText("API:", { selector: ".sr-only" })).toBeTruthy();
    expect(
      screen.queryByText("Operator:", { selector: ".sr-only" }),
    ).toBeNull();
  });

  it("shows the outcome as the fleet's bubble when a turn completes with no reply", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "deploy staging",
        reply: "",
        status: "gate_blocked",
        outcome: OUTCOME.WAITING_APPROVAL,
      }),
    ]);
    renderThread();
    expect(screen.getByText(/deploy staging/)).toBeTruthy();
    // A blocked turn does not render the instruction as if it succeeded — the
    // fleet bubble states the outcome (coding-agent finding #4).
    expect(screen.getByText(OUTCOME.WAITING_APPROVAL)).toBeTruthy();
  });

  it("shows motion but no saved-history band while connecting to an empty thread", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderThread();

    expect(
      container.querySelector('[data-connection="connecting"]'),
    ).toBeTruthy();
    expect(container.querySelector(".animate-pulse")).toBeTruthy();
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
    expect(screen.queryByText(/saved history/i)).toBeNull();
  });

  it("keeps saved history visible without adding a connection band", () => {
    const history = [
      ev({
        role: "assistant",
        actor: "fleet",
        reply: "Saved reply",
      }),
    ];
    mockStream(history, { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const connecting = renderThread();
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
    expect(screen.getByText("Saved reply")).toBeTruthy();
    connecting.unmount();

    mockStream(history, { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    const reconnecting = renderThread();
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
    expect(screen.getByText("Saved reply")).toBeTruthy();
    reconnecting.unmount();
  });

  it("offers Reconnect only after live updates stop", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    expect(screen.getByTestId("fleet-connection-notice")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeTruthy();
  });

  it("announces arrival once, and only when it was actually waiting", async () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const view = renderThread();
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    await act(async () => {
      view.rerender(threadElement());
    });

    const indicator = view.container.querySelector('[data-connection="live"]');
    expect(indicator?.getAttribute("data-arrived")).toBe("true");
    view.unmount();

    // A surface that mounts already-live announces nothing: there was no
    // wait to resolve.
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    const fresh = renderThread();
    expect(
      fresh.container
        .querySelector('[data-connection="live"]')
        ?.getAttribute("data-arrived"),
    ).toBeNull();
  });

  it("animates a turn that has started but not spoken yet", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "Howdy",
        reply: "",
        status: "received",
      }),
    ]);
    renderThread();

    // "Still working." reads the same at one second and at five minutes.
    expect(screen.getByTestId("fleet-working")).toBeTruthy();
    expect(screen.queryByText(OUTCOME.WORKING)).toBeNull();
  });

  it("keeps repeated startup failures inline in one expandable activity group", () => {
    const cause = "no instructions configured";
    mockStream(
      Array.from({ length: 15 }, (_, i) =>
        ev({
          id: `fail_${i}`,
          role: "system",
          actor: "webhook:github",
          text: "edited #541",
          reply: "",
          status: "fleet_error",
          outcome: "Failed a startup safety check",
          failureLabel: "startup_posture",
          failureDetail: cause,
        }),
      ),
    );
    renderThread();

    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    expect(screen.getByTestId("group-count").textContent).toBe("×15");
    expect(
      screen.getByText(/This fleet needs instructions before it can respond\./),
    ).toBeTruthy();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
    expect(screen.queryByText(cause)).toBeNull();
  });

  it("keeps non-actionable repeated failures compact without guidance", () => {
    mockStream(
      Array.from({ length: 3 }, (_, i) =>
        ev({
          id: `oom_${i}`,
          role: "system",
          actor: "webhook:github",
          text: "edited #541",
          reply: "",
          status: "fleet_error",
          outcome: "Ran out of memory",
          failureLabel: "oom_kill",
          failureDetail: null,
        }),
      ),
    );
    renderThread();

    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    expect(screen.getByTestId("group-count").textContent).toBe("×3");
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("animates a still-working integration delivery instead of stating an outcome", () => {
    // A received (streaming) system row has no settled outcome yet, so the
    // compact tick shows motion, not an outcome clause.
    mockStream([
      ev({
        id: "wk",
        role: "system",
        actor: "webhook:github",
        text: "opened #542",
        reply: "",
        status: "received",
        outcome: OUTCOME.WORKING,
      }),
    ]);
    const { container } = renderThread();

    const tick = container.querySelector('[data-compact="true"]');
    expect(tick?.textContent).toContain("opened #542");
    expect(tick?.textContent).not.toContain(OUTCOME.WORKING);
  });

  it("shows no banner for a single failure, and clears it once the fleet recovers", () => {
    const failure = () =>
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited #541",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
      });

    mockStream([failure()]);
    const single = renderThread();
    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    single.unmount();

    // Recovery is the case that matters: a banner that outlived it would
    // report a fleet as broken while it is working.
    mockStream([
      failure(),
      failure(),
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited #542",
        reply: "",
        status: "processed",
      }),
    ]);
    renderThread();
    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
  });
});
