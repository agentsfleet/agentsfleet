import { Schema } from "effect";

const MAX_NANOS = 9_223_372_036_854_775_807n;
const MIN_NANOS = -MAX_NANOS - 1n;
const DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const SIGNED_DECIMAL = /^(?:0|-?[1-9][0-9]*)$/;

export function isNanos(value: unknown): value is string {
  return typeof value === "string" && DECIMAL.test(value) && BigInt(value) <= MAX_NANOS;
}

/** Monetary wire values stay exact, including values beyond safe numbers. */
export const nanosSchema = Schema.String.check(Schema.makeFilter(isNanos));

/** Only the wallet balance admits debt; charges and rates remain unsigned. */
export function isSignedNanos(value: unknown): value is string {
  return typeof value === "string" && SIGNED_DECIMAL.test(value)
    && BigInt(value) >= MIN_NANOS && BigInt(value) <= MAX_NANOS;
}

export const signedNanosSchema = Schema.String.check(Schema.makeFilter(isSignedNanos));
