// The figures a run reports (tokens, wall time, cost) as each surface spells
// them: the console's status line in full, a reply's "Worked for" line the way
// Codex's transcript does. One home, so the two cannot drift.

const COUNT_FORMAT = new Intl.NumberFormat("en-US");
// Codex's `format_tokens_compact`: "12.4K".
const COMPACT_COUNT_FORMAT = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
// "41s", "1m 5s": whole units, the zero ones left out, seconds always shown.
const ELAPSED_FORMAT = new Intl.DurationFormat("en-US", { style: "narrow", secondsDisplay: "always" });
const MS_PER_SECOND = 1_000;
const SECONDS_PER_MINUTE = 60;
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

/** "41s", "1m 5s", "1h 2m 0s": how long a turn ran, to the second. */
export function formatElapsed(ms: number): string {
  const seconds = Math.round(Math.max(0, ms) / MS_PER_SECOND);
  const minutes = Math.floor(seconds / SECONDS_PER_MINUTE);
  return ELAPSED_FORMAT.format({
    hours: Math.floor(minutes / MINUTES_PER_HOUR),
    minutes: minutes % MINUTES_PER_HOUR,
    seconds: seconds % SECONDS_PER_MINUTE,
  });
}
