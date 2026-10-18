import { describe, expect, it } from "vitest";
import { MAX_RATE_NANOS, nanosToUsdPerMtok, usdPerMtokToNanos } from "./admin-model-library-types";

describe("exact catalogue money", () => {
  it.each([
    ["0", "0"], ["0.006", "6000000"], ["0.015", "15000000"], ["0.0375", "37500000"],
    ["0.0625", "62500000"], ["0.075", "75000000"], ["0.125", "125000000"], ["0.1875", "187500000"],
    ["0.000000001", "1"], ["9223372036.854775807", "9223372036854775807"],
  ])("preserves every digit from %s to %s nanos and back", (usd, nanos) => {
    expect(usdPerMtokToNanos(usd)).toBe(nanos);
    expect(nanosToUsdPerMtok(nanos)).toBe(usd);
  });
  it("normalizes decimal input without floating point", () => {
    expect(usdPerMtokToNanos("000.1500")).toBe("150000000");
    expect(nanosToUsdPerMtok("150000000")).toBe("0.15");
  });
  it.each(["", "-1", "1e3", "NaN", "Infinity", "1.0000000001", ".5", " 1", "1."])("refuses malformed decimal %s", (value) => {
    expect(() => usdPerMtokToNanos(value)).toThrow(RangeError);
    expect(() => usdPerMtokToNanos(value)).toThrow("Enter a non-negative decimal");
  });
  it.each(["", "-1", "1.5", "01", "NaN"])("refuses malformed nanos %s", (value) => {
    expect(() => nanosToUsdPerMtok(value)).toThrow(RangeError);
  });
  it("refuses amounts above the signed 64-bit ceiling", () => {
    expect(() => usdPerMtokToNanos("9223372036.854775808")).toThrow("maximum integer nanos");
    expect(() => nanosToUsdPerMtok(String(MAX_RATE_NANOS + 1n))).toThrow("maximum integer nanos");
  });
});
