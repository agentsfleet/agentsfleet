import { formatDollars } from "@/lib/money";

const EXACT_CHARGE_DECIMALS = 9;

/** A charged cost is positive; retain every recorded nanodollar in its disclosure. */
export function formatChargeAmount(nanos: string): string {
  return formatDollars(nanos, EXACT_CHARGE_DECIMALS);
}
