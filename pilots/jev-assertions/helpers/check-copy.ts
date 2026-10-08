import { expect } from "vitest";

export function checkClipboard(writeText: unknown, value: string) {
  expect(writeText).toHaveBeenCalledWith(value);
}
