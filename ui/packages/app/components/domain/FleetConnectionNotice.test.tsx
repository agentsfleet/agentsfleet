import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { CONNECTION_STATUS } from "./useFleetEventStream";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetConnectionNotice", () => {
  it("keeps history visible while connecting and reconnecting", () => {
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.CONNECTING} onRetry={vi.fn()} />,
    );
    expect(screen.getByText(/Recent history remains available/i)).toBeTruthy();
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.RECONNECTING} onRetry={vi.fn()} />,
    );
    expect(screen.getByText(/New messages will queue/i)).toBeTruthy();
  });

  it("offers a manual retry when automatic reconnects stop", async () => {
    const retry = vi.fn();
    render(<FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={retry} />);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("briefly confirms a restored connection", async () => {
    vi.useFakeTimers();
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.RECONNECTING} onRetry={vi.fn()} />,
    );
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.LIVE} onRetry={vi.fn()} />,
    );
    expect(screen.getByText("Live connection restored.")).toBeTruthy();
    await act(async () => vi.advanceTimersByTime(4_000));
    expect(screen.queryByText("Live connection restored.")).toBeNull();
  });

  it("confirms a manual recovery after the connecting state", () => {
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={vi.fn()} />,
    );
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.CONNECTING} onRetry={vi.fn()} />,
    );
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.LIVE} onRetry={vi.fn()} />,
    );
    expect(screen.getByText("Live connection restored.")).toBeTruthy();
  });

  it("clears a restored notice when the connection drops again", () => {
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.RECONNECTING} onRetry={vi.fn()} />,
    );
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.LIVE} onRetry={vi.fn()} />,
    );
    expect(screen.getByText("Live connection restored.")).toBeTruthy();
    view.rerender(
      <FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={vi.fn()} />,
    );
    expect(screen.queryByText("Live connection restored.")).toBeNull();
    expect(screen.getByText(/Live connection unavailable/i)).toBeTruthy();
  });
});
