import { expect, it } from "vitest";
import { formatElapsed, formatCount } from "./implementation";

it("checks elapsed result", () => {
  const actual = formatElapsed(59_600);
  expect(formatCount(1_234_567)).toBe("1,234,567");
});
