// The cursor trail: where a keyset feed's paging state lives.
//
// Deliberately NOT a "use client" module. Server Components read the trail out
// of `searchParams` to decide which page to fetch, and a `"use client"` file's
// exports become client references — calling one on the server would fail at
// runtime while type-checking perfectly well. The hook that writes the trail
// is the client half, in `use-url-cursor-pages.ts`.
//
// The page an operator is looking at is part of where they are, so it belongs
// in the address bar: a reload keeps the page, a pasted link opens the page
// the sender meant, and Back steps back through pages rather than leaving the
// surface entirely.

/** Query key holding the cursor trail. Short: it rides on every page link. */
export const CURSOR_TRAIL_PARAM = "c";
/** Query key binding a cursor trail to the row count that produced it. */
export const CURSOR_PAGE_SIZE_PARAM = "cps";
/** Query key holding a non-default row count. */
export const PAGE_SIZE_PARAM = "ps";

/**
 * Shared table density. Kept server-safe so the pager and the server fetch
 * boundary read the same value without importing a client module.
 */
export const DEFAULT_TABLE_PAGE_SIZE = 25;
export const TABLE_PAGE_SIZE_OPTIONS: readonly number[] = [25, 50, 100];

/** Read a trail only when it was produced with the current row count. */
export function cursorTrailFrom(
  value: string | string[] | undefined,
  pageSize: number,
  boundPageSize: string | string[] | undefined,
): string[] {
  if (value === undefined) return [];
  if (Array.isArray(boundPageSize)) return [];
  if (Number(boundPageSize) !== pageSize) return [];
  const all = Array.isArray(value) ? value : [value];
  return all.filter((entry) => entry.length > 0);
}

/** The cursor a page must fetch with, or null for the first page. */
export function cursorForTrail(trail: string[]): string | null {
  return trail[trail.length - 1] ?? null;
}

/** Accept only a supported row count; malformed or repeated input fails closed. */
export function pageSizeFrom(value: string | string[] | undefined): number {
  if (Array.isArray(value)) return DEFAULT_TABLE_PAGE_SIZE;
  const parsed = value === undefined ? Number.NaN : Number(value);
  return TABLE_PAGE_SIZE_OPTIONS.includes(parsed)
    ? parsed
    : DEFAULT_TABLE_PAGE_SIZE;
}
