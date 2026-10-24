import { formatDollars, MIN_VISIBLE_NANOS, SUBVISIBLE_AMOUNT_LABEL } from "@/lib/money";
/** Human-readable debit: never render a misleading negative zero. */
export function formatChargeAmount(nanos: string): string {
  const amount = BigInt(nanos);
  if (amount === 0n) return formatDollars(nanos);
  if (amount < MIN_VISIBLE_NANOS) return SUBVISIBLE_AMOUNT_LABEL;
  return `−${formatDollars(nanos)}`;
}
