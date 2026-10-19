import { expect, it } from "vitest";
import { nanosSchema, readNanos } from "./nanos";
import { formatDollars, compareNanos } from "../money";

// Pin tests: literal amounts prove wire bounds and integer rounding.
it.each(["0", "1", "9007199254740993", "9223372036854775807"])("preserves every digit in %s", (value) => {
  expect(nanosSchema.parse(value)).toBe(value);
  expect(readNanos(value)).toBe(value);
});
it.each([0, 1, null, undefined, true, {}, [], "", "01", "-1", "1.5", "1e3", "9223372036854775808"])(
  "refuses malformed or oversized nanos %j", (value) => {
    expect(nanosSchema.safeParse(value).success).toBe(false);
    expect(readNanos(value)).toBeNull();
  },
);
it("rounds large balances with integer arithmetic and retains sub-cent precision", () => {
  expect(formatDollars("9223372036854775807")).toBe("$9,223,372,036.8548");
  expect(formatDollars("9223372036854775807", 2)).toBe("$9,223,372,036.85");
  expect(formatDollars("999995000", 2)).toBe("$1.00");
  expect(formatDollars("50000")).toBe("$0.0001");
  expect(formatDollars("-50000")).toBe("-$0.0001");
  expect(formatDollars("-50000000")).toBe("-$0.05");
  expect(formatDollars("-9223372036854775808")).toBe("-$9,223,372,036.8548");
});
it("orders amounts beyond safe integers numerically, with missing amounts first", () => {
  expect(compareNanos("9007199254740992", "9007199254740993")).toBe(-1);
  expect(compareNanos("10", "2")).toBe(1);
  expect(compareNanos("0", "0")).toBe(0);
  expect(compareNanos(null, "0")).toBe(-1);
  expect(compareNanos("0", null)).toBe(1);
});
