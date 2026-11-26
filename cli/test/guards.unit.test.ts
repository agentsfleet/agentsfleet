import { expect, test } from "bun:test";
import { isRecord } from "../src/lib/guards.ts";

test("isRecord accepts a JSON object and refuses arrays, null and primitives", () => {
  expect(isRecord({})).toBe(true);
  expect(isRecord({ turn_id: "t" })).toBe(true);
  expect(isRecord([])).toBe(false);
  expect(isRecord([{ turn_id: "t" }])).toBe(false);
  expect(isRecord(null)).toBe(false);
  expect(isRecord("object")).toBe(false);
  expect(isRecord(1)).toBe(false);
  expect(isRecord(undefined)).toBe(false);
});
