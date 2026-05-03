import { describe, it, expect } from "vitest";
import {
  TIME_DEFAULT_LOCALE,
  TIME_INVALID_FALLBACK,
  coerceDate,
  formatTimeAbsolute,
  formatTimeRelative,
  toIso,
  visibleTimeLabel,
} from "./time-utils";

const ISO = "2026-05-03T10:00:00.000Z";
const NOW = new Date("2026-05-03T12:00:00Z");

describe("TIME_DEFAULT_LOCALE", () => {
  it("is en-US", () => {
    expect(TIME_DEFAULT_LOCALE).toBe("en-US");
  });
});

describe("coerceDate", () => {
  it("returns the same instance when given a Date", () => {
    const d = new Date(ISO);
    expect(coerceDate(d)).toBe(d);
  });
  it("parses an ISO string into a Date with the same epoch", () => {
    const d = coerceDate(ISO);
    expect(d).toBeInstanceOf(Date);
    expect(d.toISOString()).toBe(ISO);
  });
  it("returns Invalid Date for an unparseable string (caller's contract)", () => {
    const d = coerceDate("not-a-date");
    expect(d).toBeInstanceOf(Date);
    expect(Number.isNaN(d.getTime())).toBe(true);
  });
});

describe("toIso", () => {
  it("formats to canonical UTC ISO-8601 with milliseconds", () => {
    expect(toIso(new Date(ISO))).toBe(ISO);
  });
  it("normalises a non-UTC Date to a Z-suffixed UTC ISO string", () => {
    const d = new Date(Date.UTC(2026, 0, 2, 3, 4, 5, 678));
    expect(toIso(d)).toBe("2026-01-02T03:04:05.678Z");
  });
});

describe("formatTimeAbsolute", () => {
  it("uses en-US Intl.DateTimeFormat by default and matches the canonical helper output", () => {
    const expected = new Intl.DateTimeFormat("en-US", {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(ISO));
    expect(formatTimeAbsolute(ISO)).toBe(expected);
  });

  it("respects an explicit locale", () => {
    const us = formatTimeAbsolute(ISO, "en-US");
    const gb = formatTimeAbsolute(ISO, "en-GB");
    // en-US uses month-name first; en-GB uses day first → strings differ.
    expect(us).not.toBe(gb);
  });

  it("accepts a Date instance and a string and produces equal output for the same epoch", () => {
    expect(formatTimeAbsolute(new Date(ISO))).toBe(formatTimeAbsolute(ISO));
  });

  it("does not include seconds (Intl options omit them)", () => {
    expect(formatTimeAbsolute(ISO)).not.toMatch(/:\d{2}:\d{2}/);
  });
});

describe("formatTimeRelative — sub-5s 'just now' band", () => {
  it("returns 'just now' when value === now (delta = 0)", () => {
    expect(formatTimeRelative(NOW, NOW)).toBe("just now");
  });
  it("returns 'just now' for tiny positive drift (future, 3s)", () => {
    expect(formatTimeRelative(new Date(NOW.getTime() + 3_000), NOW)).toBe("just now");
  });
  it("returns 'just now' for tiny negative drift (past, 3s)", () => {
    expect(formatTimeRelative(new Date(NOW.getTime() - 3_000), NOW)).toBe("just now");
  });
  it("crosses out of 'just now' at exactly 5 s ago", () => {
    expect(formatTimeRelative(new Date(NOW.getTime() - 5_000), NOW)).toBe("5 seconds ago");
  });
});

describe("formatTimeRelative — past direction", () => {
  it.each([
    [30_000, "30 seconds ago"],
    [60_000, "1 minute ago"],
    [5 * 60_000, "5 minutes ago"],
    [60 * 60_000, "1 hour ago"],
    [3 * 60 * 60_000, "3 hours ago"],
    [24 * 60 * 60_000, "1 day ago"],
    [10 * 24 * 60 * 60_000, "10 days ago"],
    [30 * 24 * 60 * 60_000, "1 month ago"],
    [365 * 24 * 60 * 60_000, "1 year ago"],
  ])("delta=%i ms → %s", (deltaMs, expected) => {
    expect(formatTimeRelative(new Date(NOW.getTime() - deltaMs), NOW)).toBe(expected);
  });
});

describe("formatTimeRelative — future direction", () => {
  it.each([
    [30_000, "in 30 seconds"],
    [60_000, "in 1 minute"],
    [10 * 60_000, "in 10 minutes"],
    [60 * 60_000, "in 1 hour"],
    [24 * 60 * 60_000, "in 1 day"],
    [60 * 24 * 60 * 60_000, "in 2 months"],
    [2 * 365 * 24 * 60 * 60_000, "in 2 years"],
  ])("delta=+%i ms → %s", (deltaMs, expected) => {
    expect(formatTimeRelative(new Date(NOW.getTime() + deltaMs), NOW)).toBe(expected);
  });
});

describe("formatTimeRelative — singular vs plural", () => {
  it("singularises every unit when n === 1", () => {
    expect(formatTimeRelative(new Date(NOW.getTime() - 60_000), NOW)).toContain("1 minute ");
    expect(formatTimeRelative(new Date(NOW.getTime() - 60 * 60_000), NOW)).toContain("1 hour ");
    expect(formatTimeRelative(new Date(NOW.getTime() - 24 * 60 * 60_000), NOW)).toContain("1 day ");
  });
  it("pluralises when n !== 1", () => {
    expect(formatTimeRelative(new Date(NOW.getTime() - 2 * 60_000), NOW)).toContain("2 minutes");
    expect(formatTimeRelative(new Date(NOW.getTime() - 2 * 60 * 60_000), NOW)).toContain("2 hours");
  });
});

describe("formatTimeRelative — accepts string input", () => {
  it("parses an ISO string the same as a Date instance", () => {
    const past = new Date(NOW.getTime() - 90 * 60_000);
    expect(formatTimeRelative(past.toISOString(), NOW)).toBe(
      formatTimeRelative(past, NOW),
    );
  });
});

describe("formatTimeRelative — defaults", () => {
  it("uses the current wall-clock when `now` is omitted (smoke check)", () => {
    // We can't pin Date.now() here without fakeTimers, so just assert the
    // shape: a freshly created Date renders as 'just now'.
    expect(formatTimeRelative(new Date())).toBe("just now");
  });
});

describe("formatTimeAbsolute — invalid input", () => {
  // Bug this catches: Intl.DateTimeFormat.format(NaN-Date) raises RangeError
  // ("Invalid time value"). Without a guard, server components calling this
  // on junk timestamps crash the entire subtree.
  it("returns the invalid fallback for an unparseable string (no throw)", () => {
    expect(() => formatTimeAbsolute("not-a-date")).not.toThrow();
    expect(formatTimeAbsolute("not-a-date")).toBe(TIME_INVALID_FALLBACK);
  });
  it("returns the invalid fallback for a NaN-epoch Date", () => {
    expect(formatTimeAbsolute(new Date(Number.NaN))).toBe(TIME_INVALID_FALLBACK);
  });
  it("returns the invalid fallback regardless of locale", () => {
    expect(formatTimeAbsolute("garbage", "en-GB")).toBe(TIME_INVALID_FALLBACK);
  });
});

describe("formatTimeRelative — invalid input", () => {
  // Bug this catches: previously produced "NaN years ago" by cascading NaN
  // through Math.floor — silent gibberish visible to end users. Now returns
  // a deterministic fallback. If anyone removes the guard, these tests fail
  // loudly instead of shipping NaN strings.
  it("returns the invalid fallback for an unparseable string", () => {
    expect(formatTimeRelative("not-a-date", NOW)).toBe(TIME_INVALID_FALLBACK);
  });
  it("returns the invalid fallback for a NaN-epoch Date", () => {
    expect(formatTimeRelative(new Date(Number.NaN), NOW)).toBe(TIME_INVALID_FALLBACK);
  });
  it("never returns a NaN-bearing label for any malformed input shape", () => {
    for (const v of ["", "not-a-date", "2026-99-99T00:00:00Z", "🐈"]) {
      const out = formatTimeRelative(v, NOW);
      expect(out).not.toMatch(/NaN/);
    }
  });
});

describe("visibleTimeLabel", () => {
  it("'datetime' returns the iso argument verbatim", () => {
    expect(visibleTimeLabel(ISO, "datetime", "en-US", ISO)).toBe(ISO);
  });
  it("'absolute' returns formatTimeAbsolute(value, locale)", () => {
    expect(visibleTimeLabel(ISO, "absolute", "en-US", ISO)).toBe(
      formatTimeAbsolute(ISO, "en-US"),
    );
  });
  it("'relative' returns formatTimeRelative(value) using wall-clock now", () => {
    const past = new Date(Date.now() - 3 * 60_000).toISOString();
    expect(visibleTimeLabel(past, "relative", "en-US", past)).toMatch(/\d+ minutes? ago/);
  });
  it("'absolute' respects the passed locale", () => {
    expect(visibleTimeLabel(ISO, "absolute", "en-GB", ISO)).toBe(
      formatTimeAbsolute(ISO, "en-GB"),
    );
  });
});
