import { z } from "zod";

export const MAX_NANOS = 9_223_372_036_854_775_807n;
const MIN_NANOS = -MAX_NANOS - 1n;
export const nanosSchema = z.string().refine(
  (value) => /^(?:0|[1-9]\d*)$/.test(value) && BigInt(value) <= MAX_NANOS,
  "Use a decimal string of whole nanos within the maximum integer amount",
);

/** Wallet debt uses the same exact integer range with a canonical minus sign. */
export const signedNanosSchema = z.string().refine(
  (value) => /^(?:0|-?[1-9]\d*)$/.test(value) && BigInt(value) >= MIN_NANOS && BigInt(value) <= MAX_NANOS,
  "Use a signed decimal string of whole nanos within the supported integer range",
);

/** Missing or malformed stream amounts remain unknown. */
export function readNanos(value: unknown): string | null {
  const result = nanosSchema.safeParse(value);
  return result.success ? result.data : null;
}
