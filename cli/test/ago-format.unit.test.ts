// The age a table prints, across every magnitude and every shape that is not
// an age at all. The formatter reports EMPTY_CELL rather than a number
// whenever it cannot know, so a table never states something nobody measured.

import { describe, expect, test } from "bun:test";

import { ago, EMPTY_CELL } from "../src/output/format.ts";

const NOW = 1_800_000_000_000;
const SECOND_MS = 1_000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** The instant `ms` names, as the daemon would write it on the wire. */
const at = (ms: number): string => new Date(ms).toISOString();

describe("ago renders every magnitude", () => {
  test.each([
    [45 * SECOND_MS, "45s"],
    [90 * MINUTE_MS, "1h"],
    [36 * HOUR_MS, "1d"],
    [400 * DAY_MS, "1y"],
  ])("%s ms ago renders as %s", (elapsed, expected) => {
    expect(ago(at(NOW - (elapsed as number)), NOW)).toBe(expected as string);
  });

  test("each unit boundary reports the coarser unit, never the finer one", () => {
    expect(ago(at(NOW - 59 * SECOND_MS), NOW)).toBe("59s");
    expect(ago(at(NOW - MINUTE_MS), NOW)).toBe("1m");
    expect(ago(at(NOW - 59 * MINUTE_MS), NOW)).toBe("59m");
    expect(ago(at(NOW - HOUR_MS), NOW)).toBe("1h");
    expect(ago(at(NOW - 23 * HOUR_MS), NOW)).toBe("23h");
    expect(ago(at(NOW - 24 * HOUR_MS), NOW)).toBe("1d");
  });

  test("this instant reads as zero seconds, not as nothing", () => {
    expect(ago(at(NOW), NOW)).toBe("0s");
  });
});

describe("ago refuses to invent an age", () => {
  test.each([
    ["undefined", undefined],
    ["null", null],
    ["NaN", Number.NaN],
    ["a string that is not an instant", "1800000000000"],
    ["a date with no time", "2027-01-15"],
    ["an epoch-millisecond number", NOW],
    ["Infinity", Number.POSITIVE_INFINITY],
    ["an object", { created_at: NOW }],
  ])("%s renders the empty cell", (_label, value) => {
    expect(ago(value, NOW)).toBe(EMPTY_CELL);
  });

  test("an instant in the future is clock disagreement, not a negative age", () => {
    expect(ago(at(NOW + MINUTE_MS), NOW)).toBe(EMPTY_CELL);
    expect(ago(at(NOW + MINUTE_MS), NOW)).not.toContain("-");
  });
});
