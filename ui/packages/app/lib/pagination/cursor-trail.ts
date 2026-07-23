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

/**
 * Rows per page. One constant per feed so the pager's numbering and the
 * server's fetch boundary can never disagree, kept here rather than on the
 * components so a Server Component can read it without importing a client one.
 */
export const EVENTS_PAGE_SIZE = 25;
export const BILLING_PAGE_SIZE = 25;

/** Read the trail out of a `searchParams` bag, tolerating every shape it takes. */
export function cursorTrailFrom(value: string | string[] | undefined): string[] {
  if (value === undefined) return [];
  const all = Array.isArray(value) ? value : [value];
  return all.filter((entry) => entry.length > 0);
}

/** The cursor a page must fetch with, or null for the first page. */
export function cursorForTrail(trail: string[]): string | null {
  return trail[trail.length - 1] ?? null;
}
