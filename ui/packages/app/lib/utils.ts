export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function truncate(str: string, max: number): string {
  return str.length > max ? `${str.slice(0, max)}…` : str;
}

// One spelling of the ms→display rule for durations (event wall time, tool
// calls, the chat's clocks). Two surfaces grew identical private
// copies in one branch — this is the single home so the next tweak cannot
// drift them apart, and the platform's unit formatter does the spelling.
export const MS_PER_SECOND = 1_000;
const MS_PER_TENTH = 100;
const TENTHS_PER_SECOND = 10;
export const SECONDS_PER_MINUTE = 60;
const TENTHS_PER_MINUTE = SECONDS_PER_MINUTE * TENTHS_PER_SECOND;
// Seconds beside minutes always take two digits: "2m 05s".
const SECONDS_DIGITS = 2;
const DURATION_LOCALE = "en-US";
const SECONDS_FORMAT = new Intl.NumberFormat(DURATION_LOCALE, {
  style: "unit",
  unit: "second",
  unitDisplay: "narrow",
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
  useGrouping: false,
});
const MILLISECONDS_FORMAT = new Intl.NumberFormat(DURATION_LOCALE, {
  style: "unit",
  unit: "millisecond",
  unitDisplay: "narrow",
  useGrouping: false,
});

/** Wall time for a table: `850ms` under a second, `8.5s` above. */
export function formatMs(ms: number): string {
  return ms < MS_PER_SECOND ? MILLISECONDS_FORMAT.format(ms) : formatSeconds(ms);
}

/** A clock that ticks, its width held: seconds at one decimal under a minute
 * ("59.0s"), then minutes and two-digit seconds ("2m 05s"). The switch reads
 * the rounded figure, so a clock never shows "60.0s". */
export function formatSeconds(ms: number): string {
  const tenths = Math.round(Math.max(0, ms) / MS_PER_TENTH);
  if (tenths < TENTHS_PER_MINUTE) return SECONDS_FORMAT.format(tenths / TENTHS_PER_SECOND);
  const seconds = Math.floor(tenths / TENTHS_PER_SECOND);
  const padded = String(seconds % SECONDS_PER_MINUTE).padStart(SECONDS_DIGITS, "0");
  return `${Math.floor(seconds / SECONDS_PER_MINUTE)}m ${padded}s`;
}
