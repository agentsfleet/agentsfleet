import { Schema } from "effect";

const MAX_NANOS = 9_223_372_036_854_775_807n;
const DECIMAL = /^(?:0|[1-9][0-9]*)$/;

export function isNanos(value: unknown): value is string {
  return typeof value === "string" && DECIMAL.test(value) && BigInt(value) <= MAX_NANOS;
}

/** Monetary wire values stay exact, including values beyond safe numbers. */
export const nanosSchema = Schema.String.check(Schema.makeFilter(isNanos));
