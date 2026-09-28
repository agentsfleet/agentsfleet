import { describe, expect, it } from "vitest";
import { formatMs, formatSeconds } from "./utils";

describe("duration formatting", () => {
  it("test_format_seconds_fixed_width", () => {
    // Pin test: the literals are the rendered clock.
    expect(formatSeconds(400)).toBe("0.4s");
    expect(formatSeconds(8_000)).toBe("8.0s");
    expect(formatSeconds(8_500)).toBe("8.5s");
    expect(formatSeconds(125_300)).toBe("125.3s");
    expect(formatSeconds(-5)).toBe("0.0s");
  });

  it("formatMs keeps milliseconds under a second and one decimal above it", () => {
    // Pin test: the three wall-time tables render exactly these.
    expect(formatMs(850)).toBe("850ms");
    expect(formatMs(1_050)).toBe("1.1s");
    expect(formatMs(12_000)).toBe("12.0s");
    expect(formatMs(1_234_567)).toBe("1234.6s");
  });
});
