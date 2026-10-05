import { MS_PER_SECOND, SECONDS_PER_MINUTE } from "@/lib/utils";

// The run figures a reply's "Worked for" line spells, the way Codex's
// transcript does, and the rule both it and the console's status line follow:
// a figure the run did not report is left out, never drawn as zero.

const LOCALE = "en-US";
const COUNT_FORMAT = new Intl.NumberFormat(LOCALE);
// Codex's `format_tokens_compact`: "12.4K".
const COMPACT_COUNT_FORMAT = new Intl.NumberFormat(LOCALE, { notation: "compact", maximumFractionDigits: 1 });
// One narrow unit formatter per unit rather than Intl.DurationFormat, which the
// browsers this app supports (Firefox before 136, Chrome before 129) and Node
// before 23 do not have: built at module load, it would take the page with it.
const HOURS = unitFormat("hour");
const MINUTES = unitFormat("minute");
const SECONDS = unitFormat("second");
const MINUTES_PER_HOUR = 60;

/** A figure's text, or null when the run did not report it: left out, never
 * drawn as zero. */
export function runFigure(value: number | null | undefined, format: (value: number) => string): string | null {
  return value === null || value === undefined ? null : format(value);
}

/** "12,400" */
export function formatCount(value: number): string {
  return COUNT_FORMAT.format(value);
}

/** "12.4K" */
export function formatCompactCount(value: number): string {
  return COMPACT_COUNT_FORMAT.format(value);
}

/** "41s", "1m 5s", "1h 2m 3s": how long a turn ran, to the second, its zero
 * hours and minutes left out. A clock behind the server's, or no clock at all,
 * reads as no time. */
export function formatElapsed(ms: number): string {
  const seconds = Number.isFinite(ms) ? Math.round(Math.max(0, ms) / MS_PER_SECOND) : 0;
  const minutes = Math.floor(seconds / SECONDS_PER_MINUTE);
  const hours = Math.floor(minutes / MINUTES_PER_HOUR);
  const shown = [
    hours > 0 ? HOURS.format(hours) : null,
    minutes % MINUTES_PER_HOUR > 0 ? MINUTES.format(minutes % MINUTES_PER_HOUR) : null,
    SECONDS.format(seconds % SECONDS_PER_MINUTE),
  ];
  return shown.filter((unit) => unit !== null).join(" ");
}

function unitFormat(unit: "hour" | "minute" | "second"): Intl.NumberFormat {
  return new Intl.NumberFormat(LOCALE, { style: "unit", unit, unitDisplay: "narrow" });
}
