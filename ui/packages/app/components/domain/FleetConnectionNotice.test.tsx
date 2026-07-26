import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const RECONNECT_LABEL = "Reconnect";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetConnectionNotice", () => {
  it("stays silent while connecting or reconnecting", () => {
    const view = render(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.CONNECTING}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();

    view.rerender(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.RECONNECTING}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
  });

  it("stays silent while live", () => {
    render(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.LIVE}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
  });

  it("speaks only when the connection is lost, and offers the way back", async () => {
    const retry = vi.fn();
    render(<FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={retry} />);

    const notice = screen.getByTestId("fleet-connection-notice");
    expect(notice.textContent).not.toMatch(/history/i);
    expect(notice.textContent).toMatch(/Live updates stopped.*resume updates/i);

    await userEvent.click(screen.getByRole("button", { name: RECONNECT_LABEL }));
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
