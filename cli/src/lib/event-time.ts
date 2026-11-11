const INSTANT = /^\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d{1,9})?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)$/u;
const DURATION = /^(\d+)([smhd])$/u;
const UNIT_MS: Readonly<Record<string, number>> = {s: 1000, m: 60_000, h: 3_600_000, d: 86_400_000};

/** Resolve relative history windows before sending them to instant-only endpoints. */
export function eventTime(value: string, now: number, relative: boolean): string | undefined {
  if (INSTANT.test(value)) {
    const date = value.slice(0, 10);
    const calendar = Date.parse(`${date}T00:00:00Z`);
    if (!Number.isFinite(calendar) || new Date(calendar).toISOString().slice(0, 10) !== date) return undefined;
    return Number.isFinite(Date.parse(value)) ? value : undefined;
  }
  if (!relative) return undefined;
  const duration = DURATION.exec(value);
  const count = Number(duration?.[1]);
  const unit = UNIT_MS[duration?.[2] ?? ""];
  if (!Number.isSafeInteger(count) || unit === undefined) return undefined;
  const resolved = new Date(now - count * unit);
  return Number.isFinite(resolved.getTime()) && resolved.getUTCFullYear() >= 0 && resolved.getUTCFullYear() <= 9999
    ? resolved.toISOString() : undefined;
}
