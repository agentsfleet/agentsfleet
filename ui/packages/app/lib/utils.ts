export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function truncate(str: string, max: number): string {
  return str.length > max ? `${str.slice(0, max)}…` : str;
}

// One spelling of the ms→display rule for sub-minute durations (event wall
// time, tool calls, the chat's clocks). Two surfaces grew identical private
// copies in one branch — this is the single home so the next tweak cannot
// drift them apart, and the platform's unit formatter does the spelling.
const MS_PER_SECOND = 1_000;
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

/** A clock that ticks: always seconds at one decimal, so its width holds. */
export function formatSeconds(ms: number): string {
  return SECONDS_FORMAT.format(Math.max(0, ms) / MS_PER_SECOND);
}
