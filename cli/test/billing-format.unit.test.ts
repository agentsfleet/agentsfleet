import { expect, test } from "bun:test";
import { formatDollars, NANOS_PER_USD } from "../src/constants/billing.ts";

// The daemon sends nanos counters as decimal strings because they outgrow a
// JavaScript Number; the string path must keep every digit the number path loses.
test("should render a decimal-string counter beyond Number precision digit for digit", () => {
  const nanos = "123456789012345678901234567";
  expect(formatDollars(nanos)).toBe("$123,456,789,012,345,678.9012");
  expect(formatDollars(Number(nanos))).not.toBe(formatDollars(nanos));
});

test("should round half a display unit up and anything less down", () => {
  expect(formatDollars("50000")).toBe("$0.0001");
  expect(formatDollars("49999")).toBe("$0.00");
  expect(formatDollars("0")).toBe("$0.00");
});

test("should keep two decimal places and trim only the zeros beyond them", () => {
  expect(formatDollars(String(NANOS_PER_USD))).toBe("$1.00");
  expect(formatDollars("1230000000")).toBe("$1.23");
  expect(formatDollars("1234500000")).toBe("$1.2345");
});

test("should render a number or an absent counter through the same currency shape", () => {
  expect(formatDollars(1_230_000_000)).toBe(formatDollars("1230000000"));
  expect(formatDollars(null)).toBe("$0.00");
  expect(formatDollars(undefined)).toBe("$0.00");
});
