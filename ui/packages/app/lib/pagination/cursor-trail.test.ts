import { describe, expect, it } from "vitest";

import { cursorForTrail, cursorTrailFrom } from "./cursor-trail";

describe("cursorTrailFrom", () => {
  it("reads an empty trail from a missing param", () => {
    expect(cursorTrailFrom(undefined)).toEqual([]);
  });

  it("reads a one-cursor trail from a single string", () => {
    expect(cursorTrailFrom("tok_1")).toEqual(["tok_1"]);
  });

  it("reads the full trail from a repeated param (?c=…&c=…)", () => {
    // Next hands a repeated query key back as a string[], which is the shape
    // a multi-page trail always takes.
    expect(cursorTrailFrom(["tok_1", "tok_2", "tok_3"])).toEqual(["tok_1", "tok_2", "tok_3"]);
  });

  it("drops empty entries so a stray `?c=` cannot forge a page", () => {
    expect(cursorTrailFrom(["tok_1", "", "tok_2"])).toEqual(["tok_1", "tok_2"]);
    expect(cursorTrailFrom("")).toEqual([]);
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
