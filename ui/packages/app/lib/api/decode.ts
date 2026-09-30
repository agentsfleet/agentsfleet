// The guards every decoder in this directory narrows wire JSON with. A response
// crosses a network boundary, so nothing about its shape is trusted until one
// of these has checked it.

export const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

export const isNonEmptyString = (value: unknown): value is string =>
  typeof value === "string" && value.trim().length > 0;

/** An epoch-millisecond timestamp the backend sent as a JSON integer. */
export const isEpochMs = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value);

/** The items of a one-page list, each checked by `decode`. The page's
 * `next_cursor` is always null on these routes, so there is nothing to walk. */
export function decodeOnePage<T>(value: unknown, decode: (item: unknown) => T): T[] {
  if (!isRecord(value) || !Array.isArray(value.items)) {
    throw new Error("list response omitted items");
  }
  return value.items.map(decode);
}
