import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { CONNECTION_STATUS } from "./useFleetEventStream";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetConnectionNotice", () => {
  it("stays silent for every connection state that resolves itself", () => {
    // Connecting, reconnecting and live all resolve without the operator
    // doing anything, and the header indicator already shows them with
    // motion. A band above someone's conversation is for decisions.
    for (const status of [
      CONNECTION_STATUS.CONNECTING,
      CONNECTION_STATUS.RECONNECTING,
      CONNECTION_STATUS.LIVE,
    ] as const) {
      const view = render(<FleetConnectionNotice status={status} onRetry={vi.fn()} />);
      expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
      view.unmount();
    }
  });

  it("speaks only when the connection is lost, and offers the way back", async () => {
    const retry = vi.fn();
    render(<FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={retry} />);

    const notice = screen.getByTestId("fleet-connection-notice");
    // No claim about history: the operator is looking at it.
    expect(notice.textContent).not.toMatch(/history/i);
    expect(notice.textContent).toMatch(/Live updates stopped/i);

    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("clears itself the moment the connection comes back", () => {
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={vi.fn()} />,
    );
    expect(screen.getByTestId("fleet-connection-notice")).toBeTruthy();

    view.rerender(<FleetConnectionNotice status={CONNECTION_STATUS.LIVE} onRetry={vi.fn()} />);
    // Recovery is announced by the indicator's arrival cue, not by a second
    // band that outstays the news.
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
  });
});
