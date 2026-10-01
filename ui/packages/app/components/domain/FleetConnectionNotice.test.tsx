import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const RECONNECT_LABEL = "Retry now";
const NOTICE = "fleet-connection-notice";

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
    expect(screen.queryByTestId(NOTICE)).toBeNull();

    view.rerender(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.RECONNECTING}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId(NOTICE)).toBeNull();
  });

  it("stays silent while live", () => {
    render(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.LIVE}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId(NOTICE)).toBeNull();
  });

  it("speaks only when the connection is lost, and offers the way back", async () => {
    const retry = vi.fn();
    render(<FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={retry} />);

    const notice = screen.getByTestId(NOTICE);
    // A warning for a state that heals itself, still spoken as it arrives.
    expect(notice.getAttribute("role")).toBe("alert");
    expect(notice.className).toMatch(/\btext-warning\b/);
    expect(notice.className).not.toMatch(/destructive/);
    expect(notice.textContent).not.toMatch(/history/i);
    expect(notice.textContent).toMatch(/temporarily unavailable.*reconnecting automatically/i);

    await userEvent.click(screen.getByRole("button", { name: RECONNECT_LABEL }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("says access is gone once the stream is revoked, and offers no retry", () => {
    render(<FleetConnectionNotice status={CONNECTION_STATUS.REVOKED} onRetry={vi.fn()} />);

    const notice = screen.getByTestId(NOTICE);
    // pin test: literal is the contract — the sentence a removed member reads.
    expect(notice.textContent).toBe("You no longer have access to this workspace.");
    expect(notice.getAttribute("role")).toBe("alert");
    expect(screen.queryByRole("button", { name: RECONNECT_LABEL })).toBeNull();
  });

  it("clears itself the moment the connection comes back", () => {
    const view = render(
      <FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={vi.fn()} />,
    );
    expect(screen.getByTestId(NOTICE)).toBeTruthy();

    view.rerender(<FleetConnectionNotice status={CONNECTION_STATUS.LIVE} onRetry={vi.fn()} />);
    // Recovery is announced by the indicator's arrival cue, not by a second
    // band that outstays the news.
    expect(screen.queryByTestId(NOTICE)).toBeNull();
  });
});
