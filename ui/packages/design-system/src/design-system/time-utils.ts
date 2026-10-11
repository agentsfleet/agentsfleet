/*
 * Pure date-formatting helpers shared by the <Time> client component AND
 * server-side callers (e.g. badges that surface a formatted timestamp in
 * a `title=` attribute). Kept in a directive-free module so server
 * imports don't drag the client-component entry point onto the server
 * bundle.
 */

export type TimeFormat = "absolute" | "relative" | "datetime" | "clock";

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
): string {
  const d = coerceDate(value);
  if (Number.isNaN(d.getTime())) return TIME_INVALID_FALLBACK;
  return formatterFor(locale, ABSOLUTE_FORMAT, ABSOLUTE_OPTIONS).format(d);
}

export function formatTimeClock(
  value: string | Date,
  locale: string = DEFAULT_LOCALE,
): string {
  const d = coerceDate(value);
  if (Number.isNaN(d.getTime())) return TIME_INVALID_FALLBACK;
  return formatterFor(locale, CLOCK_FORMAT, CLOCK_OPTIONS).format(d);
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
): string {
  if (format === "datetime") return iso;
  if (format === "relative") return formatTimeRelative(value);
  if (format === "clock") return formatTimeClock(value, locale);
  return formatTimeAbsolute(value, locale);
}

export { DEFAULT_LOCALE as TIME_DEFAULT_LOCALE };
