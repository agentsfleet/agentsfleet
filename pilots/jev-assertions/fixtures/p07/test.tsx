import { expect, it } from "vitest";
import { formatElapsed } from "./implementation";

it("checks elapsed result", () => {
  const actual = formatElapsed(59_600);
  expect(actual).toMatch(/^[0-9hms ]+$/);
});
