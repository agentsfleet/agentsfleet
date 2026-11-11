import {expect, it} from "vitest";
import {clamp, take} from "./range";

it("bounds untrusted numeric reveal counts without negative slicing or extra rows", () => {
  const rows = ["one", "two"];
  expect(take(rows, NaN)).toEqual([]);
  expect(take(rows, -1)).toEqual([]);
  expect(take(rows, 1.9)).toEqual(["one"]);
  expect(take(rows, Infinity)).toEqual(rows);
  expect(take([], 1)).toEqual([]);
  expect(clamp(3, 1, 0)).toBe(0);
});
