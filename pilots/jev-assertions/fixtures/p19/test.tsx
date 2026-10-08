import { expect, it } from "vitest";
import { formatTimeRelative, coerceDate } from "./implementation";

const NOW = new Date("2026-05-03T12:00:00Z");

it("checks relative result", () => {
  const actual = formatTimeRelative(new Date(NOW.getTime() - 5_000), NOW);
  expect(coerceDate(NOW).getTime()).toBe(NOW.getTime());
});
