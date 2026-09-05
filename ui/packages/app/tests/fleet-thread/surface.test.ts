import { REFRESH_SETTLE_SLACK_MS, ev, mockStream, onRunCompletedMock, renderThread, renderThreadWithInitial, routerRefreshMock, serverEvent, threadElement } from "./harness";
import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { REFRESH_DEBOUNCE_MS } from "@/components/domain/useRefreshOnCompletion";
import { FleetThread } from "@/components/domain/FleetThread";
import { CONNECTION_STATUS } from "@/components/domain/useFleetEventStream";

describe("FleetThread — empty state", () => {
  it("renders the waiting-for-activity hint when no events", () => {
    mockStream([]);
    renderThread();
    expect(
      screen.getByText(/Message this fleet or wait for its next trigger/i),
    ).toBeTruthy();
    expect(screen.queryByText(/0 events/i)).toBeNull();
  });
});

describe("FleetThread — header chrome", () => {
  it("shows one Chat heading and one aligned Live indicator", () => {
    mockStream([
      ev({ role: "system", actor: "config_reload", text: "Reloaded" }),
    ]);
    const { container } = renderThread();
    const header = container.querySelector('[data-testid="fleet-chat-header"]');
    expect(header).toBeTruthy();
    expect(header?.className).toMatch(/justify-between/);
    expect(screen.getByRole("heading", { name: "Chat" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Steer" })).toBeNull();
    expect(screen.queryByText(/1 events/)).toBeNull();
    const liveStatus = screen.getByLabelText("Connection status: Live");
    expect(liveStatus.className).toMatch(/text-pulse/);
    expect(liveStatus.querySelector('[aria-hidden="true"]')?.className).toMatch(
      /bg-current/,
    );
  });

  it("uses one destructive colour for the Offline label and dot", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    const offlineStatus = screen.getByLabelText("Connection status: Not live");
    expect(offlineStatus.className).toMatch(/text-destructive/);
    expect(
      offlineStatus.querySelector('[aria-hidden="true"]')?.className,
    ).toMatch(/bg-current/);
  });

  it("keeps the composer in the static transcript footer", () => {
    mockStream([]);
    const { container } = renderThread();
    const transcript = container.querySelector('[aria-label="Fleet chat"]');
    const composer = container.querySelector('[aria-label="Chat composer"]');
    expect(transcript).toBeTruthy();
    expect(composer).toBeTruthy();
    expect(transcript?.contains(composer)).toBe(true);
    expect(composer?.getAttribute("id")).toBe("fleet-steer-composer");
    const footer = container.querySelector('[data-testid="fleet-chat-footer"]');
    expect(footer?.contains(composer)).toBe(true);
    expect(footer?.className).toMatch(/max-w-6xl/);
    expect(footer?.className).toMatch(/shrink-0/);
    expect(container.querySelector('[role="log"]')?.contains(composer)).toBe(
      false,
    );
    expect(
      screen.getByRole("button", { name: /jump to latest/i }).className,
    ).toMatch(/absolute/);
  });

  it("names each connection state rather than only the live one", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    renderThread();
    expect(screen.getByText(/^Reconnecting…$/)).toBeTruthy();
  });

  it("says a lost feed is not live, and keeps the composer usable", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    expect(screen.getByText(/^Not live$/)).toBeTruthy();
    const input = screen.getByPlaceholderText(
      /message this fleet/i,
    ) as HTMLTextAreaElement;
    expect(input.disabled).toBe(false);
  });
});

describe("FleetThread — summary refresh", () => {
  it("does not signal a completion for terminal events already present in the server snapshot", () => {
    mockStream([]);
    renderThreadWithInitial([serverEvent()]);

    expect(onRunCompletedMock).not.toHaveBeenCalled();
    expect(routerRefreshMock).not.toHaveBeenCalled();
  });

  it("signals a completion once when a live event completes, and never touches the router", async () => {
    const received = ev({
      id: "event-refresh",
      role: "assistant",
      actor: "fleet",
      status: "received",
    });
    mockStream([received], { isRunning: true });
    const view = renderThread();
    expect(onRunCompletedMock).not.toHaveBeenCalled();

    mockStream([{ ...received, status: "processed" }]);
    view.rerender(threadElement());

    // Debounced: a completion burst refreshes the summary once, not once per
    // frame — so the wait outlasts that window.
    await waitFor(() => expect(onRunCompletedMock).toHaveBeenCalledTimes(1), {
      timeout: REFRESH_DEBOUNCE_MS + REFRESH_SETTLE_SLACK_MS,
    });
    view.rerender(threadElement());
    expect(onRunCompletedMock).toHaveBeenCalledTimes(1);
    // The whole-page re-render per completion is the cost this shape retired.
    expect(routerRefreshMock).not.toHaveBeenCalled();
  });
});

describe("FleetThread — fluid composer", () => {
  it("keeps the message field and its send action available while running", () => {
    mockStream(
      [
        ev({
          role: "assistant",
          actor: "fleet",
          text: "streaming…",
          status: "received",
        }),
      ],
      { isRunning: true },
    );
    renderThread();
    const input = screen.getByPlaceholderText(
      /message this fleet/i,
    ) as HTMLTextAreaElement;
    expect(input.disabled).toBe(false);
    // A working fleet is not a reason to park a message in the browser: the
    // send action stays live and nothing announces a queue.
    expect(screen.getByRole("button", { name: /^Send/ })).toBeTruthy();
    expect(screen.queryByText(/will queue/i)).toBeNull();
  });

  it("uses the idle placeholder when not running", () => {
    mockStream([], { isRunning: false });
    renderThread();
    expect(screen.getByPlaceholderText(/message this fleet/i)).toBeTruthy();
  });
});

describe("FleetThread — connection-state header", () => {
  it("renders the Reconnecting badge while connectionStatus=RECONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    renderThread();
    expect(screen.getAllByText(/Reconnecting…/).length).toBeGreaterThan(0);
  });

  it("renders the Connecting badge while connectionStatus=CONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    renderThread();
    expect(screen.getByText(/Connecting…/)).toBeTruthy();
  });
});
