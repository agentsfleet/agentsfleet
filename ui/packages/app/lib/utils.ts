export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function truncate(str: string, max: number): string {
  return str.length > max ? `${str.slice(0, max)}…` : str;
}

// One spelling of the ms→display rule for sub-minute durations (tool calls,
// event wall time). Two surfaces grew identical private copies in one branch —
// this is the single home so the next tweak cannot drift them apart.
const MS_PER_SECOND = 1_000;
export function formatMs(ms: number): string {
  return ms < MS_PER_SECOND ? `${ms}ms` : `${(ms / MS_PER_SECOND).toFixed(1)}s`;
}
