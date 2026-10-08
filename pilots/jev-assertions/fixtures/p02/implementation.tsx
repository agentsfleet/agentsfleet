/*
 * Pure date-formatting helpers shared by the <Time> client component AND
 * server-side callers (e.g. badges that surface a formatted timestamp in
 * a `title=` attribute). Kept in a directive-free module so server
 * imports don't drag the client-component entry point onto the server
 * bundle.
 */

const TIME_RELATIVE = "relative";
const TIME_DATETIME = "datetime";
const TIME_CLOCK = "clock";
const TWO_DIGIT = "2-digit";
const SECONDS_PER_MINUTE = 60;

export type TimeFormat = "absolute" | typeof TIME_RELATIVE | typeof TIME_DATETIME | typeof TIME_CLOCK;

const DEFAULT_LOCALE = "en-US";

const ABSOLUTE_OPTIONS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "short",
  day: TWO_DIGIT,
  hour: TWO_DIGIT,
  minute: TWO_DIGIT,
};

/* Time of day alone, to the second. For surfaces where every entry shares the
 * day and the date would be repeated noise — a conversation thread, a run's
 * latest outcome — while the second still matters for ordering two events. */
const CLOCK_OPTIONS: Intl.DateTimeFormatOptions = {
  hour: TWO_DIGIT,
  minute: TWO_DIGIT,
  second: TWO_DIGIT,
};

/**
 * Fallback string for invalid timestamp inputs. Matches the visible "—"
 * the <Time> component renders when `coerceDate(value)` produces NaN.
 */
export const TIME_INVALID_FALLBACK = "—";

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
  return new Intl.DateTimeFormat(locale, ABSOLUTE_OPTIONS).format(d);
}

export function formatTimeClock(
  value: string | Date,
  locale: string = DEFAULT_LOCALE,
): string {
  const d = coerceDate(value);
  if (Number.isNaN(d.getTime())) return TIME_INVALID_FALLBACK;
  return new Intl.DateTimeFormat(locale, CLOCK_OPTIONS).format(d);
}

export function formatTimeRelative(
  value: string | Date,
  now: Date = new Date(),
): string {
  const d = coerceDate(value);
  if (Number.isNaN(d.getTime())) return TIME_INVALID_FALLBACK;
  const deltaSec = Math.round((d.getTime() - now.getTime()) / 1000);
  const abs = Math.abs(deltaSec);

  if (abs < 5) return "just now";

  const past = deltaSec < 0;
  const unit =
    abs < SECONDS_PER_MINUTE ? { n: abs, label: "second" }
    : abs < 3_600 ? { n: Math.floor(abs / SECONDS_PER_MINUTE), label: "minute" }
    : abs < 86_400 ? { n: Math.floor(abs / 3_600), label: "hour" }
    : abs < 2_592_000 ? { n: Math.floor(abs / 86_400), label: "day" }
    : abs < 31_536_000 ? { n: Math.floor(abs / 2_592_000), label: "month" }
    : { n: Math.floor(abs / 31_536_000), label: "year" };

  const noun = unit.n === 1 ? unit.label : `${unit.label}s`;
  return past ? `${unit.n} ${noun} ago` : `in ${unit.n} ${noun}`;
}

export function visibleTimeLabel(
  value: string | Date,
  format: TimeFormat,
  locale: string,
  iso: string,
): string {
  if (format === TIME_DATETIME) return iso;
  if (format === TIME_RELATIVE) return formatTimeRelative(value);
  if (format === TIME_CLOCK) return formatTimeClock(value, locale);
  return formatTimeAbsolute(value, locale);
}

export { DEFAULT_LOCALE as TIME_DEFAULT_LOCALE };
