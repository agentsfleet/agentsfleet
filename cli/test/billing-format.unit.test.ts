import { expect, test } from "bun:test";
import { formatDollars, formatRate, NANOS_PER_USD } from "../src/constants/billing.ts";

test("should render aggregated decimal amounts beyond Number precision digit for digit", () => {
  const nanos = "123456789012345678901234567";
  expect(formatDollars(nanos)).toBe("$123,456,789,012,345,678.9012");
});

test("should round half a display unit up and anything less down", () => {
  expect(formatDollars("50000")).toBe("$0.0001");
  expect(formatDollars("49999")).toBe("$0.00");
  expect(formatDollars("0")).toBe("$0.00");
});

test.each([
  ["-50000000", "-$0.05"], ["-50000", "-$0.0001"],
  ["-50", ">-$0.0001"], ["-9223372036854775808", "-$9,223,372,036.8548"],
])("shows debt %s without rounding it to zero", (amount, expected) => {
  expect(formatDollars(amount)).toBe(expected);
});

test("should keep two decimal places and trim only the zeros beyond them", () => {
  expect(formatDollars(String(NANOS_PER_USD))).toBe("$1.00");
  expect(formatDollars("1230000000")).toBe("$1.23");
  expect(formatDollars("1234500000")).toBe("$1.2345");
});

test("model rates preserve all nine fractional digits and exact large values", () => {
  expect(formatRate("1")).toBe("$0.000000001");
  expect(formatRate("0")).toBe("$0.00");
  expect(formatRate("3625000")).toBe("$0.003625");
  expect(formatRate("9223372036854775807")).toBe("$9,223,372,036.854775807");
});
