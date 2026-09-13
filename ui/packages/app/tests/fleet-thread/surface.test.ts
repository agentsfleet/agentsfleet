import { ev, mockStream, renderThread, renderThreadWithInitial, routerRefreshMock, serverEvent, threadElement } from "./harness";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
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
  // The panel header is gone in the steady state. It said "Chat" directly
  // beneath a tab reading "Chat", and a "Live" that was true on almost every
  // load — chrome restating what the operator can already see. The states that
  // need explaining still render it; see the connection tests.
  it("shows no chat heading and no live badge once the stream is fine", () => {
    mockStream([
      ev({ role: "system", actor: "config_reload", text: "Reloaded" }),
    ]);
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="fleet-chat-header"]')).toBeNull();
    expect(screen.queryByRole("heading", { name: "Chat" })).toBeNull();
    expect(screen.queryByLabelText("Connection status: Live")).toBeNull();
    expect(screen.queryByRole("link", { name: "Steer" })).toBeNull();
    expect(screen.queryByText(/1 events/)).toBeNull();
    // The transcript keeps its accessible name, which is the one place the
    // word "Chat" still earns its keep.
    expect(container.querySelector('[aria-label="Chat"][role="log"]')).toBeTruthy();
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
    expect(footer?.className).toMatch(/max-w-measure/);
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

describe("FleetThread — the thread never re-runs the page", () => {
  it("a live completion leaves the router alone: the strip reads the stream, the thread its rows", () => {
    const received = ev({
      id: "event-refresh",
      role: "assistant",
      actor: "fleet",
      status: "received",
    });
    mockStream([received], { isRunning: true });
    const view = renderThread();
    mockStream([{ ...received, status: "processed" }]);
    view.rerender(threadElement());
    renderThreadWithInitial([serverEvent()]);
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
