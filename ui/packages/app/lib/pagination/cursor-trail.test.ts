import { describe, expect, it } from "vitest";

import {
  DEFAULT_TABLE_PAGE_SIZE,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "./cursor-trail";

describe("cursorTrailFrom", () => {
  it("reads an empty trail from a missing param", () => {
    expect(cursorTrailFrom(undefined, 25, "25")).toEqual([]);
  });

  it("reads a one-cursor trail from a single string", () => {
    expect(cursorTrailFrom("tok_1", 25, "25")).toEqual(["tok_1"]);
  });

  it("reads the full trail from a repeated param (?c=…&c=…)", () => {
    // Next hands a repeated query key back as a string[], which is the shape
    // a multi-page trail always takes.
    expect(cursorTrailFrom(["tok_1", "tok_2", "tok_3"], 25, "25")).toEqual([
      "tok_1",
      "tok_2",
      "tok_3",
    ]);
  });

  it("drops empty entries so a stray `?c=` cannot forge a page", () => {
    expect(cursorTrailFrom(["tok_1", "", "tok_2"], 25, "25")).toEqual([
      "tok_1",
      "tok_2",
    ]);
    expect(cursorTrailFrom("", 25, "25")).toEqual([]);
  });

  it("drops cursors created with a different row count", () => {
    expect(cursorTrailFrom(["tok_1", "tok_2"], 100, "25")).toEqual([]);
    expect(cursorTrailFrom("tok_1", 25, undefined)).toEqual([]);
    expect(cursorTrailFrom("tok_1", 25, ["25", "25"])).toEqual([]);
  });
});

describe("cursorForTrail", () => {
  it("has no cursor for the first page", () => {
    expect(cursorForTrail([])).toBeNull();
  });

  it("fetches with the last cursor walked", () => {
    expect(cursorForTrail(["tok_1", "tok_2"])).toBe("tok_2");
  });
});

describe("pageSizeFrom", () => {
  it("should accept each supported table page size", () => {
    expect(pageSizeFrom("25")).toBe(25);
    expect(pageSizeFrom("50")).toBe(50);
    expect(pageSizeFrom("100")).toBe(100);
  });

  it("should reject missing, malformed, repeated, and unsupported values", () => {
    expect(pageSizeFrom(undefined)).toBe(DEFAULT_TABLE_PAGE_SIZE);
    expect(pageSizeFrom("abc")).toBe(DEFAULT_TABLE_PAGE_SIZE);
    expect(pageSizeFrom("0")).toBe(DEFAULT_TABLE_PAGE_SIZE);
    expect(pageSizeFrom("26")).toBe(DEFAULT_TABLE_PAGE_SIZE);
    expect(pageSizeFrom(["25", "100"])).toBe(DEFAULT_TABLE_PAGE_SIZE);
  });
});
