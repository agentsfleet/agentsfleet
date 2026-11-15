import {describe, expect, it, vi} from "vitest";
import {formatTimeAbsoluteWithZone, formatTimeCompact, formatTimeMinute, formatTimeRange, nextLocalMidnight,
  previousReportingMonthEnd, sameCalendarDay, TIME_INVALID_FALLBACK, visibleTimeLabel} from "./time-utils";

const INSTANT = "2026-11-14T13:30:00Z";
const INVALID = "not-a-date";

describe("shared local time formatting", () => {
  it("shows the same instant as 7pm in India and 8:30am in New York", () => {
    expect(formatTimeMinute(INSTANT, "en-US", "Asia/Kolkata")).toBe("7:00 PM");
    expect(formatTimeCompact(INSTANT, "en-US", "Asia/Kolkata")).toBe("Nov 14, 7:00 PM");
    expect(formatTimeMinute(INSTANT, "en-US", "America/New_York")).toBe("8:30 AM");
    expect(formatTimeCompact("2026-11-14T00:30:00Z", "en-US", "America/New_York")).toBe("Nov 13, 7:30 PM");
  });

  it("uses the browser calendar by default and supports another locale", () => {
    expect(formatTimeMinute(INSTANT)).toBe("1:30 PM");
    expect(formatTimeCompact(new Date(INSTANT))).toBe("Nov 14, 1:30 PM");
    expect(formatTimeMinute(INSTANT, "en-GB")).toBe("13:30");
    expect(formatTimeCompact(INSTANT, "en-GB")).toBe("14 Nov, 13:30");
    expect(formatTimeRange(INSTANT, INSTANT)).toBe("Nov 14, 2026");
    expect(formatTimeRange(INSTANT, INSTANT, "en-GB")).toBe("14 Nov 2026");
  });

  it("converts dates across midnight and handles daylight-saving jumps", () => {
    expect(formatTimeRange("2026-11-14T20:30:00Z", "2026-11-15T10:30:00Z", "en-US", "Asia/Kolkata")).toBe("Nov 15, 2026");
    expect(formatTimeRange("2026-11-14T20:30:00Z", "2026-11-15T10:30:00Z", "en-US", "America/New_York")).toBe("Nov 14 – 15, 2026");
    expect(formatTimeMinute("2026-03-08T06:30:00Z", "en-US", "America/New_York")).toBe("1:30 AM");
    expect(formatTimeMinute("2026-03-08T07:30:00Z", "en-US", "America/New_York")).toBe("3:30 AM");
  });

  it.each([[INVALID, INSTANT], [INSTANT, INVALID], ["2026-11-15T00:00:00Z", INSTANT]])(
    "keeps invalid and reversed ranges from crashing (%s, %s)", (from, through) => {
      expect(formatTimeRange(from, through)).toBe(TIME_INVALID_FALLBACK);
    },
  );

  it("keeps invalid instants, locales and timezones on the shared fallback", () => {
    expect(formatTimeMinute(INVALID)).toBe(TIME_INVALID_FALLBACK);
    expect(formatTimeCompact(INVALID)).toBe(TIME_INVALID_FALLBACK);
    expect(formatTimeMinute(INSTANT, "en-US", "bad/timezone")).toBe(TIME_INVALID_FALLBACK);
    expect(formatTimeRange(INSTANT, INSTANT, "en-US", "bad/timezone")).toBe(TIME_INVALID_FALLBACK);
    expect(formatTimeRange(INSTANT, INSTANT, "bad_locale")).toBe(TIME_INVALID_FALLBACK);
  });

  it("uses the common formats through Time's public format selector", () => {
    expect(visibleTimeLabel(INSTANT, "minute", "en-US", INSTANT, "Asia/Kolkata")).toBe("7:00 PM");
    expect(visibleTimeLabel(INSTANT, "compact", "en-US", INSTANT, "Asia/Kolkata")).toBe("Nov 14, 7:00 PM");
    expect(visibleTimeLabel("2026-11-14T20:30:00Z", "date", "en-US", INSTANT, "Asia/Kolkata")).toBe("Nov 15, 2026");
    expect(visibleTimeLabel(INVALID, "date", "en-US", "")).toBe(TIME_INVALID_FALLBACK);
    expect(visibleTimeLabel(INSTANT, "month", "en-US", INSTANT)).toBe("November 2026");
    expect(visibleTimeLabel(INVALID, "month", "en-US", "")).toBe(TIME_INVALID_FALLBACK);
  });

  it("keeps timezone names in the shared absolute tooltip", () => {
    expect(formatTimeAbsoluteWithZone(INSTANT)).toBe("Nov 14, 2026, 01:30 PM · UTC");
    const original = Intl.DateTimeFormat.prototype.resolvedOptions;
    const options = vi.spyOn(Intl.DateTimeFormat.prototype, "resolvedOptions").mockImplementation(function (this: Intl.DateTimeFormat) {
      return {...original.call(this), timeZone: ""};
    });
    try {expect(formatTimeAbsoluteWithZone(INSTANT)).toContain(" · Local time");} finally {options.mockRestore();}
  });
});

describe("shared calendar boundaries", () => {
  it("groups equal local dates and distinguishes month, year and day changes", () => {
    expect(sameCalendarDay(INSTANT, "2026-11-14T19:00:00+05:30")).toBe(true);
    expect(sameCalendarDay(INSTANT, "2026-11-15T00:00:00Z")).toBe(false);
    expect(sameCalendarDay(INSTANT, "2026-12-14T13:30:00Z")).toBe(false);
    expect(sameCalendarDay(INSTANT, "2027-11-14T13:30:00Z")).toBe(false);
    expect(sameCalendarDay(INVALID, INSTANT)).toBe(false);
    expect(sameCalendarDay(INSTANT, INVALID)).toBe(false);
    expect(new Date(nextLocalMidnight(Date.parse(INSTANT))).toISOString()).toBe("2026-11-15T00:00:00.000Z");
  });

  it("keeps historical accounting periods stable across year and leap-year boundaries", () => {
    expect(previousReportingMonthEnd(INSTANT)).toBe("2026-10-31T23:59:59.999Z");
    expect(previousReportingMonthEnd("2026-01-09T00:00:00Z")).toBe("2025-12-31T23:59:59.999Z");
    expect(previousReportingMonthEnd("2024-03-01T00:00:00Z")).toBe("2024-02-29T23:59:59.999Z");
    expect(() => previousReportingMonthEnd(INVALID)).toThrow(RangeError);
  });
});
