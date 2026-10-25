import { describe, expect, it } from "vitest";
import { formatTimeCompactUtc, formatTimeRangeUtc, TIME_INVALID_FALLBACK } from "./time-utils";

const REPORT_START = "2026-10-01T00:00:00Z";
const REPORT_END = "2026-10-25T23:59:59Z";
const INVALID_DATE = "not-a-date";

describe("UTC reporting dates", () => {
  it("keeps a period's UTC dates when source offsets cross midnight", () => {
    const label = formatTimeRangeUtc("2026-09-30T17:00:00-07:00", "2026-10-26T05:29:59+05:30");
    expect(label).toBe("Oct 1 – 25, 2026");
  });

  it("formats a compact activity in the period's UTC timezone", () => {
    expect(formatTimeCompactUtc("2026-10-26T00:15:00+05:30")).toBe("Oct 25, 18:45");
    expect(formatTimeCompactUtc(new Date("2026-10-25T18:45:00Z"))).toBe("Oct 25, 18:45");
  });

  it.each([[INVALID_DATE, REPORT_END], [REPORT_START, INVALID_DATE], [REPORT_END, REPORT_START]])(
    "shows the shared fallback for an invalid or reversed range (%s, %s)", (from, through) => {
      expect(formatTimeRangeUtc(from, through)).toBe(TIME_INVALID_FALLBACK);
    },
  );

  it("does not throw while formatting an invalid activity", () => {
    expect(formatTimeCompactUtc(INVALID_DATE)).toBe(TIME_INVALID_FALLBACK);
  });

  it("supports the caller's locale while preserving UTC", () => {
    expect(formatTimeCompactUtc("2026-10-26T00:15:00+05:30", "en-GB")).toBe("25 Oct, 18:45");
    expect(formatTimeRangeUtc(REPORT_START, REPORT_END, "en-GB")).toBe("1 – 25 Oct 2026");
  });
});
