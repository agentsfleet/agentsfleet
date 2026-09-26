import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

import { FleetThought, THOUGHT_LABEL, THOUGHT_LIVE_LABEL, latestSentence, type FleetThoughtProps } from "./FleetThought";

const NOW = 5_000;
const STARTED = 1_000;
const ENDED = 9_500;
const TICK_MS = 1_000;
const REASONING = "Reading the diff. Checking the header";

let parentRenders = 0;

function Reply(props: Omit<FleetThoughtProps, "children">) {
  parentRenders += 1;
  return (
    <FleetThought {...props}>
      <p>{props.reasoning}</p>
    </FleetThought>
  );
}

function chip() {
  return screen.getByRole("button");
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  parentRenders = 0;
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FleetThought", () => {
  it("test_thought_chip_live", () => {
    render(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    expect(chip().textContent).toContain(THOUGHT_LIVE_LABEL);
    // Pin test: elapsed since the row's start stamp, at tenths.
    expect(chip().textContent).toContain("4.0s");
    expect(chip().textContent).toContain("Checking the header");
    expect(chip().querySelector("[data-braille-spinner]")).toBeTruthy();
    // Open while it streams: the thought itself is on screen.
    expect(screen.getByText(REASONING, { selector: "p" })).toBeTruthy();
  });

  it("test_thought_chip_folds_with_duration", () => {
    const view = render(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    view.rerender(<Reply live={false} reasoning={REASONING} startedAtMs={STARTED} endedAtMs={ENDED} />);
    expect(chip().textContent).toBe(`${THOUGHT_LABEL} · 8.5s`);
    expect(chip().querySelector("[data-braille-spinner]")).toBeNull();
    expect(screen.queryByText(REASONING, { selector: "p" })).toBeNull();

    // An operator-opened chip stays open through later renders.
    fireEvent.click(chip());
    expect(screen.getByText(REASONING, { selector: "p" })).toBeTruthy();
    view.rerender(<Reply live={false} reasoning={`${REASONING}.`} startedAtMs={STARTED} endedAtMs={ENDED} />);
    expect(screen.getByText(`${REASONING}.`, { selector: "p" })).toBeTruthy();
  });

  it("test_thought_clock_ticks_only_the_leaf", () => {
    const view = render(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    const rendersBefore = parentRenders;
    act(() => {
      vi.advanceTimersByTime(TICK_MS);
    });
    expect(chip().textContent).toContain("5.0s");
    // The tick re-rendered the clock and nothing that holds the reply.
    expect(parentRenders).toBe(rendersBefore);

    view.rerender(<Reply live={false} reasoning={REASONING} startedAtMs={STARTED} endedAtMs={ENDED} />);
    expect(vi.getTimerCount()).toBe(0);
    view.rerender(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("test_thought_clock_resumes_from_row_stamp", () => {
    // A remount mid-thought reads the row's start, not zero.
    const view = render(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    view.unmount();
    render(<Reply live reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    expect(chip().textContent).toContain("4.0s");
    cleanup();
    // A reply recovered from detail has no stamp: it folds with no duration.
    render(<Reply live={false} reasoning={REASONING} startedAtMs={null} endedAtMs={null} />);
    expect(chip().textContent).toBe(THOUGHT_LABEL);
    cleanup();
    // Live without a start: no clock rather than a clock from zero.
    render(<Reply live reasoning={REASONING} startedAtMs={null} endedAtMs={null} />);
    expect(chip().textContent).toContain(THOUGHT_LIVE_LABEL);
    expect(chip().textContent).not.toContain("·");
    cleanup();
    // Folded with a start but no recorded end: no duration is invented.
    render(<Reply live={false} reasoning={REASONING} startedAtMs={STARTED} endedAtMs={null} />);
    expect(chip().textContent).toBe(THOUGHT_LABEL);
  });

  it("test_latest_sentence_edges", () => {
    expect(latestSentence("")).toBe("");
    // A tail of nothing but whitespace has no sentence to show.
    expect(latestSentence("   ")).toBe("");
    expect(latestSentence("no stop")).toBe("no stop");
    expect(latestSentence("A. B.  ")).toBe("B.");
    expect(latestSentence("Ünï. Ça va")).toBe("Ça va");
    // Only the tail is scanned; the last sentence is still found.
    expect(latestSentence(`${"Earlier thought. ".repeat(200)}Final check`)).toBe("Final check");
  });
});
