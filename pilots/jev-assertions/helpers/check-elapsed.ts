import { expect } from "vitest";

const EXPECTED = "1m 0s";
export function checkElapsed(value: string) {
  expect(value).toBe(EXPECTED);
}
