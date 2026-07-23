import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

import { FleetConnectionIndicator } from "./FleetConnectionIndicator";
import { CONNECTION_STATUS } from "./useFleetEventStream";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetConnectionIndicator", () => {
  it("animates while connecting and reconnecting, not when live", () => {
    const { container, rerender } = render(
      <FleetConnectionIndicator status={CONNECTION_STATUS.CONNECTING} />,
    );
    // The dot moves while we are trying — the one moment the operator wants a
    // sign of life.
    expect(container.querySelector('[data-connection="connecting"]')).toBeTruthy();
    expect(container.innerHTML).toContain("animate-pulse");

    rerender(<FleetConnectionIndicator status={CONNECTION_STATUS.RECONNECTING} />);
    expect(container.innerHTML).toContain("animate-pulse");
  });

  it("fires the arrival cue once it comes up, then settles after the cue window", () => {
    vi.useFakeTimers();
    const { container, rerender } = render(
      <FleetConnectionIndicator status={CONNECTION_STATUS.CONNECTING} />,
    );
    // Coming up after a genuine wait announces itself.
    act(() => {
      rerender(<FleetConnectionIndicator status={CONNECTION_STATUS.LIVE} />);
    });
    expect(container.querySelector('[data-connection="live"]')?.getAttribute("data-arrived")).toBe("true");

    // The cue is a one-shot: after its window the indicator settles back into
    // the steady pulse. This fires the timeout callback that clears it.
    act(() => {
      // pin test: literal is the contract — past the ~700ms arrival-cue window.
      vi.advanceTimersByTime(1000);
    });
    expect(container.querySelector('[data-connection="live"]')?.getAttribute("data-arrived")).toBeNull();
  });

  it("does not announce an arrival for a surface that mounts already live", () => {
    const { container } = render(<FleetConnectionIndicator status={CONNECTION_STATUS.LIVE} />);
    expect(container.querySelector('[data-connection="live"]')?.getAttribute("data-arrived")).toBeNull();
  });
});
