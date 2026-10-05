import { describe, expect, it, vi } from "vitest";

import { formatCompactCount, formatCount, formatElapsed, runFigure } from "./run-figures-format";

// Pin tests throughout: each literal is the string a surface draws.

describe("run-figures-format", () => {
  it("should leave a figure the run did not report out, and keep a reported zero", () => {
    expect(runFigure(null, formatCount)).toBeNull();
    expect(runFigure(undefined, formatCount)).toBeNull();
    // A reported zero is a figure, not a missing one.
    expect(runFigure(0, formatCount)).toBe("0");
    expect(runFigure(12_400, formatCompactCount)).toBe("12.4K");
  });

  it("should count in full for the status line and compactly for a reply", () => {
    expect(formatCount(1_234_567)).toBe("1,234,567");
    expect(formatCompactCount(999)).toBe("999");
    expect(formatCompactCount(12_449)).toBe("12.4K");
    expect(formatCompactCount(1_250_000)).toBe("1.3M");
  });

  it("should spell elapsed time to the second, as Intl's narrow duration does", () => {
    expect(formatElapsed(0)).toBe("0s");
    // A browser clock behind the server's reads as no time, never a throw.
    expect(formatElapsed(-5_000)).toBe("0s");
    expect(formatElapsed(41_000)).toBe("41s");
    // Rounds to the nearest second: 59.6s reads as a minute.
    expect(formatElapsed(59_600)).toBe("1m 0s");
    expect(formatElapsed(65_000)).toBe("1m 5s");
    expect(formatElapsed(3_723_000)).toBe("1h 2m 3s");
    expect(formatElapsed(7_200_000)).toBe("2h 0s");
    // No clock at all (an unparseable instant) reads as no time, never a throw.
    expect(formatElapsed(Number.NaN)).toBe("0s");
    expect(formatElapsed(Number.POSITIVE_INFINITY)).toBe("0s");
  });

  it("test_elapsed_needs_no_duration_format", async () => {
    // Stands in for Firefox before 136: the API is simply not there.
    const original = Object.getOwnPropertyDescriptor(Intl, "DurationFormat");
    Reflect.deleteProperty(Intl, "DurationFormat");
    try {
      vi.resetModules();
      const fresh = await import("./run-figures-format");
      expect(fresh.formatElapsed(65_000)).toBe("1m 5s");
    } finally {
      if (original !== undefined) Object.defineProperty(Intl, "DurationFormat", original);
    }
  });
});
