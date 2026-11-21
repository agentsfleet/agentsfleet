import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ChatProgressLabel } from "./ChatProgressLabel";

afterEach(() => {cleanup(); vi.useRealTimers();});

// Value: protects=live feedback without repeated screen-reader announcements;
// fails_when=queued work lacks a spinner or the clock stops after a minute; seam=clock; no_claim=provider progress.
it.each(["Queued", "Thinking", "Saving"])("ticks whole seconds for %s with a stable spoken label", label => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-11-14T12:00:00Z"));
  const {container, unmount} = render(<ChatProgressLabel label={label} />);
  expect(screen.getByText("0s").getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByText(label).getAttribute("data-chat-loading")).toBe("true");
  expect(container.querySelector("[data-chat-spinner]")).toBeTruthy();
  act(() => {vi.advanceTimersByTime(65_000);});
  expect(screen.getByText("65s").getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByText(label)).toBeTruthy();
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it("uses the recorded start and clamps future starts", () => {
  vi.useFakeTimers(); vi.setSystemTime(100_000);
  const {rerender} = render(<ChatProgressLabel label="Queued" startedAtMs={98_000} />);
  expect(screen.getByText("2s")).toBeTruthy();
  rerender(<ChatProgressLabel label="Thinking" startedAtMs={101_000} />);
  expect(screen.getByText("0s")).toBeTruthy();
});
