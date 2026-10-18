import { z } from "zod";

// The guards every decoder in this directory narrows wire JSON with. A response
// crosses a network boundary, so nothing about its shape is trusted until one
// of these has checked it.

export const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

export const isNonEmptyString = (value: unknown): value is string =>
  typeof value === "string" && value.trim().length > 0;

/** An instant as the daemon writes it: RFC 3339 in UTC (`2026-10-17T18:06:49.123Z`,
 * `2026-10-17T18:06:49Z` for a whole second). Not fixed-width, so compare parsed values,
 * never the text. A number is refused. */
export const Instant = z.iso.datetime();

export const isInstant = (value: unknown): value is string => Instant.safeParse(value).success;

/** Orders two instants by the moment each names. Their text does not sort as
 * time, because a whole second drops its fraction. */
export const byInstant = (a: string, b: string): number => Date.parse(a) - Date.parse(b);

/** The epoch millisecond a wire instant names, or null for anything that is not
 * one: a frame field read off a live stream is untrusted until checked. */
export const instantMs = (value: unknown): number | null => (isInstant(value) ? Date.parse(value) : null);

/** The items of a one-page list, each checked by `decode`. The page's
 * `next_cursor` is always null on these routes, so there is nothing to walk. */
export function decodeOnePage<T>(value: unknown, decode: (item: unknown) => T): T[] {
  if (!isRecord(value) || !Array.isArray(value.items)) {
    throw new Error("list response omitted items");
  }
  return value.items.map(decode);
}
