/*
 * Pure date-formatting helpers shared by the <Time> client component AND
 * server-side callers (e.g. badges that surface a formatted timestamp in
 * a `title=` attribute). Kept in a directive-free module so server
 * imports don't drag the client-component entry point onto the server
 * bundle.
 */

export type TimeFormat = "absolute" | "relative" | "datetime";

const DEFAULT_LOCALE = "en-US";

const ABSOLUTE_OPTIONS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "short",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};

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
  return new Intl.DateTimeFormat(locale, ABSOLUTE_OPTIONS).format(d);
}

export function formatTimeRelative(
  value: string | Date,
  now: Date = new Date(),
): string {
  const d = coerceDate(value);
  const deltaSec = Math.round((d.getTime() - now.getTime()) / 1000);
  const abs = Math.abs(deltaSec);

  if (abs < 5) return "just now";

  const past = deltaSec < 0;
  const unit =
    abs < 60 ? { n: abs, label: "second" }
    : abs < 3_600 ? { n: Math.floor(abs / 60), label: "minute" }
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
  if (format === "datetime") return iso;
  if (format === "relative") return formatTimeRelative(value);
  return formatTimeAbsolute(value, locale);
}

export { DEFAULT_LOCALE as TIME_DEFAULT_LOCALE };
