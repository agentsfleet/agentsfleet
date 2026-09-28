import { describe, expect, it } from "vitest";
import { formatMs, formatSeconds } from "./utils";

describe("duration formatting", () => {
  it("test_format_seconds_fixed_width", () => {
    // Pin test: the literals are the rendered clock.
    expect(formatSeconds(400)).toBe("0.4s");
    expect(formatSeconds(8_000)).toBe("8.0s");
    expect(formatSeconds(8_500)).toBe("8.5s");
    expect(formatSeconds(125_300)).toBe("2m 05s");
    expect(formatSeconds(-5)).toBe("0.0s");
  });

  it("test_durations_show_minutes", () => {
    // Pin test: the literals are the rendered durations.
    expect(formatMs(125_000)).toBe("2m 05s");
    expect(formatMs(59_000)).toBe("59.0s");
    expect(formatMs(850)).toBe("850ms");
    expect(formatSeconds(59_960)).toBe("1m 00s");
    expect(formatSeconds(3_600_000)).toBe("60m 00s");
  });

  it("formatMs keeps milliseconds under a second and one decimal above it", () => {
    // Pin test: the three wall-time tables render exactly these.
    expect(formatMs(850)).toBe("850ms");
    expect(formatMs(1_050)).toBe("1.1s");
    expect(formatMs(12_000)).toBe("12.0s");
    expect(formatMs(1_234_567)).toBe("20m 34s");
  });
});
