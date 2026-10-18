import { NANOS_PER_USD } from "./types";

const NANO_DIGITS = 9;
const MIN_DECIMALS = 2;
const HALF = 2n;
const INTEGER_FORMATTER = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

/** Round in integer nanos, preserving cents even above Number.MAX_SAFE_INTEGER. */
export function formatDollars(nanos: string, decimals = 4): string {
  const nanosPerUnit = 10n ** BigInt(NANO_DIGITS - decimals);
  const unitsPerUsd = BigInt(NANOS_PER_USD) / nanosPerUnit;
  const units = (BigInt(nanos) + nanosPerUnit / HALF) / nanosPerUnit;
  const fraction = (units % unitsPerUsd).toString().padStart(decimals, "0")
    .replace(/0+$/, "").padEnd(MIN_DECIMALS, "0");
  return `$${INTEGER_FORMATTER.format(units / unitsPerUsd)}.${fraction}`;
}

/** Numeric order for decimal strings; an absent amount sorts before a known one. */
export function compareNanos(a: string | null, b: string | null): number {
  const left = a === null ? -1n : BigInt(a);
  const right = b === null ? -1n : BigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}
