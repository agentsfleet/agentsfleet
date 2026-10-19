import { describe, expect, test } from "bun:test";
import { byInstant } from "../src/lib/instant.ts";

const WHOLE = "2026-10-17T18:06:49Z";
const WHOLE_MILLIS = "2026-10-17T18:06:49.000Z";
const EARLIER_MILLIS = "2026-10-17T18:06:49.009Z";
const LATER_MILLIS = "2026-10-17T18:06:49.01Z";
const QUARTER = "2026-10-17T18:06:49.25Z";

describe("instant ordering", () => {
  test("treats different precisions of the same instant as equal and preserves their input order", () => {
    expect(byInstant(WHOLE, WHOLE_MILLIS)).toBe(0);
    expect(byInstant(WHOLE_MILLIS, WHOLE)).toBe(0);
    expect([WHOLE, WHOLE_MILLIS].sort(byInstant)).toEqual([WHOLE, WHOLE_MILLIS]);
  });

  test("breaks a same-second tie by milliseconds across fractional widths", () => {
    expect(byInstant(LATER_MILLIS, EARLIER_MILLIS)).toBe(1);
    expect(byInstant(EARLIER_MILLIS, LATER_MILLIS)).toBe(-1);
  });

  test("puts a later instant after an earlier one even when its text sorts first", () => {
    expect(QUARTER < WHOLE).toBe(true);
    expect(byInstant(QUARTER, WHOLE)).toBeGreaterThan(0);
    expect([QUARTER, LATER_MILLIS, WHOLE, EARLIER_MILLIS, WHOLE_MILLIS].sort(byInstant))
      .toEqual([WHOLE, WHOLE_MILLIS, EARLIER_MILLIS, LATER_MILLIS, QUARTER]);
  });
});
