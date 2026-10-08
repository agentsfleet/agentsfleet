import { expect } from "vitest";

const EXPECTED = "text-eyebrow text-muted-foreground";
export function checkClasses(value: string) {
  expect(value).toBe(EXPECTED);
}
