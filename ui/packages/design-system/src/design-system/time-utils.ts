/*
 * Pure date-formatting helpers shared by the <Time> client component AND
 * server-side callers (e.g. badges that surface a formatted timestamp in
 * a `title=` attribute). Kept in a directive-free module so server
 * imports don't drag the client-component entry point onto the server
 * bundle.
 */

export type TimeFormat = "absolute" | "relative" | "datetime" | "clock" | "date" | "month" | "compact" | "minute";

const DEFAULT_LOCALE = "en-US";

const ABSOLUTE_OPTIONS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "short",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};

/* Time of day alone, to the second. For surfaces where every entry shares the
 * day and the date would be repeated noise — a conversation thread, a run's
 * latest outcome — while the second still matters for ordering two events. */
const CLOCK_OPTIONS: Intl.DateTimeFormatOptions = {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
};

/**
 * Fallback string for invalid timestamp inputs. Matches the visible "—"
 * the <Time> component renders when `coerceDate(value)` produces NaN.
 */
export const TIME_INVALID_FALLBACK = "—";

/* Built once. A table re-renders every relative-time cell on a sort click or a
 * pending flip, and each one formats its tooltip; constructing a formatter per
 * call made that a few hundred constructions per click. Only a caller that
 * names another locale pays for one. */
const ABSOLUTE_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, ABSOLUTE_OPTIONS);
const CLOCK_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, CLOCK_OPTIONS);
const DATE_OPTIONS: Intl.DateTimeFormatOptions = {month: "short", day: "numeric", year: "numeric"};
const COMPACT_OPTIONS: Intl.DateTimeFormatOptions = {month: "short", day: "numeric", hour: "numeric", minute: "2-digit"};
const MINUTE_OPTIONS: Intl.DateTimeFormatOptions = {hour: "numeric", minute: "2-digit"};
const DATE_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, DATE_OPTIONS);
const COMPACT_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, COMPACT_OPTIONS);
const MINUTE_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, MINUTE_OPTIONS);
const UTC_MONTH_OPTIONS: Intl.DateTimeFormatOptions = {month: "long", year: "numeric", timeZone: "UTC"};
const UTC_MONTH_FORMAT = new Intl.DateTimeFormat(DEFAULT_LOCALE, UTC_MONTH_OPTIONS);

function formatterFor(
  locale: string,
  built: Intl.DateTimeFormat,
  options: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  return locale === DEFAULT_LOCALE ? built : new Intl.DateTimeFormat(locale, options);
}

export function coerceDate(value: string | Date): Date {
  return value instanceof Date ? value : new Date(value);
}

export function toIso(d: Date): string {
  return d.toISOString();
}

export function formatTimeAbsolute(
  value: string | Date,
  locale: string = DEFAULT_LOCALE,
  timeZone?: string,
): string {
  return formatLocalDate(value, locale, timeZone, ABSOLUTE_OPTIONS, ABSOLUTE_FORMAT);
}

export function formatTimeClock(
  value: string | Date,
  locale: string = DEFAULT_LOCALE,
  timeZone?: string,
): string {
  return formatLocalDate(value, locale, timeZone, CLOCK_OPTIONS, CLOCK_FORMAT);
}

/** A report's instants in the reader's calendar, including local date rollovers. */
export function formatTimeRange(from: string | Date, through: string | Date, locale = DEFAULT_LOCALE, timeZone?: string): string {
  const start = coerceDate(from), end = coerceDate(through);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime()) || end < start) return TIME_INVALID_FALLBACK;
  try {
    const formatter = timeZone ? new Intl.DateTimeFormat(locale, {...DATE_OPTIONS, timeZone}) : formatterFor(locale, DATE_FORMAT, DATE_OPTIONS);
    return formatter.formatRange(start, end).replace(/\s/g, " ");
  } catch {return TIME_INVALID_FALLBACK;}
}

export function formatTimeCompact(value: string | Date, locale = DEFAULT_LOCALE, timeZone?: string): string {
  return formatLocalDate(value, locale, timeZone, COMPACT_OPTIONS, COMPACT_FORMAT);
}

export function formatTimeMinute(value: string | Date, locale = DEFAULT_LOCALE, timeZone?: string): string {
  return formatLocalDate(value, locale, timeZone, MINUTE_OPTIONS, MINUTE_FORMAT);
}

function formatLocalDate(value: string | Date, locale: string, timeZone: string | undefined, options: Intl.DateTimeFormatOptions, built: Intl.DateTimeFormat): string {
  const date = coerceDate(value);
  if (Number.isNaN(date.getTime())) return TIME_INVALID_FALLBACK;
  try {
    return (timeZone ? new Intl.DateTimeFormat(locale, {...options, timeZone}) : formatterFor(locale, built, options)).format(date);
  } catch {return TIME_INVALID_FALLBACK;}
}

/* The unit factors the relative label steps through. A month is 30 days and a
 * year 365, which is all a "3 months ago" label needs. */
export const MS_PER_SECOND = 1_000;
export const SECONDS_PER_MINUTE = 60;
export const SECONDS_PER_HOUR = 60 * SECONDS_PER_MINUTE;
export const SECONDS_PER_DAY = 24 * SECONDS_PER_HOUR;
export const SECONDS_PER_MONTH = 30 * SECONDS_PER_DAY;
export const SECONDS_PER_YEAR = 365 * SECONDS_PER_DAY;

export function formatTimeRelative(
  value: string | Date,
  now: Date = new Date(),
): string {
  const d = coerceDate(value);
  if (Number.isNaN(d.getTime())) return TIME_INVALID_FALLBACK;
  const deltaSec = Math.round((d.getTime() - now.getTime()) / MS_PER_SECOND);
  const abs = Math.abs(deltaSec);

  if (abs < 5) return "just now";

  const past = deltaSec < 0;
  const unit =
    abs < SECONDS_PER_MINUTE ? { n: abs, label: "second" }
    : abs < SECONDS_PER_HOUR ? { n: Math.floor(abs / SECONDS_PER_MINUTE), label: "minute" }
    : abs < SECONDS_PER_DAY ? { n: Math.floor(abs / SECONDS_PER_HOUR), label: "hour" }
    : abs < SECONDS_PER_MONTH ? { n: Math.floor(abs / SECONDS_PER_DAY), label: "day" }
    : abs < SECONDS_PER_YEAR ? { n: Math.floor(abs / SECONDS_PER_MONTH), label: "month" }
    : { n: Math.floor(abs / SECONDS_PER_YEAR), label: "year" };

  const noun = unit.n === 1 ? unit.label : `${unit.label}s`;
  return past ? `${unit.n} ${noun} ago` : `in ${unit.n} ${noun}`;
}

export function visibleTimeLabel(
  value: string | Date,
  format: TimeFormat,
  locale: string,
  iso: string,
  timeZone?: string,
): string {
  if (format === "datetime") return iso;
  if (format === "relative") return formatTimeRelative(value);
  if (format === "clock") return formatTimeClock(value, locale, timeZone);
  if (format === "compact") return formatTimeCompact(value, locale, timeZone);
  if (format === "minute") return formatTimeMinute(value, locale, timeZone);
  if (format === "date") return formatLocalDate(value, locale, timeZone, DATE_OPTIONS, DATE_FORMAT);
  if (format === "month") {
    const date = coerceDate(value);
    const formatter = formatterFor(locale, UTC_MONTH_FORMAT, UTC_MONTH_OPTIONS);
    return Number.isNaN(date.getTime()) ? TIME_INVALID_FALLBACK : formatter.format(date);
  }
  return formatTimeAbsolute(value, locale, timeZone);
}

export { DEFAULT_LOCALE as TIME_DEFAULT_LOCALE };

/** Calendar grouping follows the reader's day rather than the timestamp's spelling. */
export function sameCalendarDay(left: string | Date, right: string | Date): boolean {
  const a = coerceDate(left), b = coerceDate(right);
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** Local midnight uses calendar arithmetic so daylight-saving days can vary in length. */
export function nextLocalMidnight(now: number): number {
  const date = new Date(now);date.setHours(24, 0, 0, 0);return date.getTime();
}

/** Billing period identities follow the API's calendar, independent of display timezone. */
export function previousReportingMonthEnd(value: string | Date): string {
  const date = coerceDate(value);
  return new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1) - 1).toISOString();
}

export function formatTimeAbsoluteWithZone(value: string | Date, locale = DEFAULT_LOCALE): string {
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone || "Local time";
  return `${formatTimeAbsolute(value, locale)} · ${zone}`;
}
