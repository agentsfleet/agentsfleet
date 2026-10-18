import { z } from "zod";

export const MAX_NANOS = 9_223_372_036_854_775_807n;
export const nanosSchema = z.string().refine(
  (value) => /^(?:0|[1-9]\d*)$/.test(value) && BigInt(value) <= MAX_NANOS,
  "Use a decimal string of whole nanos within the maximum integer amount",
);

/** Missing or malformed stream amounts remain unknown. */
export function readNanos(value: unknown): string | null {
  const result = nanosSchema.safeParse(value);
  return result.success ? result.data : null;
}
