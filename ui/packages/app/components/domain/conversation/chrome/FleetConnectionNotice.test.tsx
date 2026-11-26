import { afterEach, describe, expect, it, vi } from "vitest";
import { createRef } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FleetConnectionNotice } from "./FleetConnectionNotice";
import { CONNECTION_STATUS } from "../stream/useFleetTurnStream";

const RECONNECT_LABEL = "Reconnect";
const NOTICE = "fleet-connection-notice";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetConnectionNotice", () => {
  it("waits for an interruption, then shows the pending reconnect", () => {
    const view = render(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.CONNECTING}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId(NOTICE)).toBeNull();
    const announcement = screen.getByRole("status");
    expect(announcement.textContent).toBe("");

    view.rerender(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.RECONNECTING}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.getByRole("status")).toBe(announcement);
    expect(announcement.textContent).toBe("Reconnecting live updates.");
    expect(screen.getByRole("button", {name: "Reconnecting…"}).getAttribute("aria-disabled")).toBe("true");
  });

  it("stays silent while live", () => {
    render(
      <FleetConnectionNotice
        status={CONNECTION_STATUS.LIVE}
        onRetry={vi.fn()}
      />,
    );
    expect(screen.queryByTestId(NOTICE)).toBeNull();
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("announces the paused updates and offers a reconnect", async () => {
    const retry = vi.fn();
    render(<FleetConnectionNotice status={CONNECTION_STATUS.OFFLINE} onRetry={retry} />);

    const notice = screen.getByTestId(NOTICE);
    expect(notice.textContent).toContain("Live updates paused");
    expect(screen.getByRole("status").textContent).toBe("Live updates paused");

    await userEvent.click(screen.getByRole("button", { name: RECONNECT_LABEL }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("says access is gone once the stream is revoked, and offers no retry", () => {
    render(<FleetConnectionNotice status={CONNECTION_STATUS.REVOKED} onRetry={vi.fn()} />);

    const notice = screen.getByTestId(NOTICE);
    expect(notice.textContent).toMatch(/no longer have access/);
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

  it("holds focus on retry while reconnecting and restores the draft input when live", async () => {
    const input = createRef<HTMLTextAreaElement>();
    const retry = vi.fn();
    const renderState = (status: typeof CONNECTION_STATUS.OFFLINE | typeof CONNECTION_STATUS.RECONNECTING | typeof CONNECTION_STATUS.LIVE) => <>
      <FleetConnectionNotice status={status} onRetry={retry} focusTarget={input} />
      <textarea aria-label="Draft" ref={input} defaultValue="Keep my draft" />
    </>;
    const view = render(renderState(CONNECTION_STATUS.OFFLINE));
    await userEvent.click(screen.getByRole("button", {name: RECONNECT_LABEL}));
    view.rerender(renderState(CONNECTION_STATUS.RECONNECTING));
    const busy = screen.getByRole("button", {name: "Reconnecting…"});
    expect(document.activeElement).toBe(busy);
    await userEvent.keyboard("{Enter}");
    expect(retry).toHaveBeenCalledOnce();
    view.rerender(renderState(CONNECTION_STATUS.LIVE));
    expect(document.activeElement).toBe(input.current);
    expect(input.current?.value).toBe("Keep my draft");
    expect(screen.getByRole("status").textContent).toBe("Live updates restored.");
  });

  it("leaves the draft alone after focus moves away from Reconnect", async () => {
    const input = createRef<HTMLTextAreaElement>();
    const onRetry = vi.fn();
    const renderState = (status: typeof CONNECTION_STATUS.OFFLINE | typeof CONNECTION_STATUS.LIVE) => <>
      <FleetConnectionNotice status={status} onRetry={onRetry} focusTarget={input} />
      {status === CONNECTION_STATUS.OFFLINE ? <button type="button">Dismiss notice</button> : null}
      <textarea aria-label="Draft" ref={input} defaultValue="Keep my draft" />
    </>;
    const view = render(renderState(CONNECTION_STATUS.OFFLINE));
    await userEvent.click(screen.getByRole("button", {name: RECONNECT_LABEL}));
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", {name: "Dismiss notice"}));
    view.rerender(renderState(CONNECTION_STATUS.LIVE));
    expect(document.activeElement).toBe(document.body);
    expect(input.current?.value).toBe("Keep my draft");
  });
});
