import { describe, expect, it } from "vitest";
import { byInstant, Instant, instantMs, isInstant } from "./decode";

// The daemon's own spellings: a whole second drops its fraction, and a
// fraction is the shortest that names the millisecond.
const WHOLE = "2026-10-17T18:06:49Z";
const QUARTER = "2026-10-17T18:06:49.25Z";
const WHOLE_MS = Date.UTC(2026, 9, 17, 18, 6, 49);

describe("Instant", () => {
  it("accepts the daemon's RFC 3339 instants, whole and fractional", () => {
    expect(isInstant(WHOLE)).toBe(true);
    expect(isInstant(QUARTER)).toBe(true);
    expect(Instant.parse(QUARTER)).toBe(QUARTER);
  });

  it("refuses an epoch-millisecond number", () => {
    expect(isInstant(WHOLE_MS)).toBe(false);
    expect(Instant.safeParse(WHOLE_MS).success).toBe(false);
  });

  it("refuses a date with no time and text that names no moment", () => {
    expect(isInstant("2026-10-17")).toBe(false);
    expect(isInstant("not-an-instant")).toBe(false);
    expect(isInstant(null)).toBe(false);
  });
});

describe("byInstant", () => {
  it("orders by the moment, where text order would put the later instant first", () => {
    // "…49.25Z" sorts before "…49Z" as text ('.' < 'Z'), though it is later.
    expect(QUARTER < WHOLE).toBe(true);
    expect(byInstant(QUARTER, WHOLE)).toBeGreaterThan(0);
    expect([QUARTER, WHOLE].sort(byInstant)).toEqual([WHOLE, QUARTER]);
    expect(byInstant(WHOLE, WHOLE)).toBe(0);
  });
});

describe("instantMs", () => {
  it("reads an instant as the epoch millisecond it names", () => {
    expect(instantMs(WHOLE)).toBe(WHOLE_MS);
    expect(instantMs(QUARTER)).toBe(WHOLE_MS + 250);
  });

  it("reads anything else as no instant, a number included", () => {
    expect(instantMs(WHOLE_MS)).toBeNull();
    expect(instantMs("bogus")).toBeNull();
    expect(instantMs(undefined)).toBeNull();
  });
});
