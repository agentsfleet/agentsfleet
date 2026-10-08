import { expect, it } from "vitest";
import { formatElapsed } from "./implementation";

it("checks elapsed result", () => {
  const actual = formatElapsed(59_600);
  expect(actual).toBe("1m 0s");
});
