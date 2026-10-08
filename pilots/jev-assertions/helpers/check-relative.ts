import { expect } from "vitest";

const EXPECTED = "5 seconds ago";
export function checkRelative(value: string) {
  expect(value).toBe(EXPECTED);
}
