import { NANOS_PER_USD } from "./types";

const NANO_DIGITS = 9;
const MIN_DECIMALS = 2;
const HALF = 2n;
const INTEGER_FORMATTER = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });
export const MIN_VISIBLE_NANOS = 50_000n;
export const SUBVISIBLE_AMOUNT_LABEL = "<$0.0001";
const SUBVISIBLE_DEBT_LABEL = ">-$0.0001";

/** Round in integer nanos, preserving cents even above Number.MAX_SAFE_INTEGER. */
export function formatDollars(nanos: string, decimals = 4): string {
  const nanosPerUnit = 10n ** BigInt(NANO_DIGITS - decimals);
  const unitsPerUsd = BigInt(NANOS_PER_USD) / nanosPerUnit;
  const amount = BigInt(nanos);
  const magnitude = amount < 0n ? -amount : amount;
  const units = (magnitude + nanosPerUnit / HALF) / nanosPerUnit;
  const fraction = (units % unitsPerUsd).toString().padStart(decimals, "0")
    .replace(/0+$/, "").padEnd(MIN_DECIMALS, "0");
  return `${amount < 0n ? "-" : ""}$${INTEGER_FORMATTER.format(units / unitsPerUsd)}.${fraction}`;
}

/** A small remaining credit or debt must stay distinguishable from zero. */
export function formatBalance(nanos: string, decimals = 4): string {
  const amount = BigInt(nanos);
  const magnitude = amount < 0n ? -amount : amount;
  if (magnitude > 0n && magnitude < MIN_VISIBLE_NANOS) {
    return amount < 0n ? SUBVISIBLE_DEBT_LABEL : SUBVISIBLE_AMOUNT_LABEL;
  }
  return formatDollars(nanos, decimals);
}

/** Numeric order for decimal strings; an absent amount sorts before a known one. */
export function compareNanos(a: string | null, b: string | null): number {
  const left = a === null ? -1n : BigInt(a);
  const right = b === null ? -1n : BigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}
