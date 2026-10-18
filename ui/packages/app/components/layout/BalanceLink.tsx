import Link from "next/link";
import { cn } from "@agentsfleet/design-system";
import {
  formatDollars,
  MIN_VISIBLE_NANOS,
  SUBVISIBLE_AMOUNT_LABEL,
} from "@/app/(dashboard)/settings/billing/lib/charges";
import { NANOS_PER_USD } from "@/lib/types";

// Header balance links to Billing. Information blue separates the amount
// from navigation; exhaustion keeps the destructive treatment.
export const BALANCE_HREF = "/settings/billing";
export const BALANCE_LABEL = "Credits";
export const BALANCE_ARIA_LABEL = "Credit balance";
export const BALANCE_EXHAUSTED_ARIA_LABEL = "Credit balance exhausted";

const CENTS_PER_USD = 100;
const NANOS_PER_CENT = NANOS_PER_USD / CENTS_PER_USD;
const CENTS_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/**
 * Cents in the header, because $4.8066 is a figure to parse and $4.81 is one
 * to read.
 *
 * Two floors below that, and they are the charges table's own floors, for the
 * same reason: a live balance must never render as $0.00, which is the one
 * figure that says "spent". Under a cent it keeps the billing page's
 * four-decimal precision; under what four decimals can show, it says so,
 * exactly as a sub-visible debit does.
 */
export function formatHeaderBalance(nanos: number): string {
  if (nanos > 0 && nanos < MIN_VISIBLE_NANOS) return SUBVISIBLE_AMOUNT_LABEL;
  if (nanos > 0 && nanos < NANOS_PER_CENT) return formatDollars(nanos);
  return CENTS_FORMATTER.format(nanos / NANOS_PER_USD);
}

export function BalanceLink({
  balanceNanos,
  isExhausted,
}: {
  balanceNanos: number;
  isExhausted: boolean;
}) {
  return (
    <Link
      href={BALANCE_HREF}
      aria-label={isExhausted ? BALANCE_EXHAUSTED_ARIA_LABEL : BALANCE_ARIA_LABEL}
      data-exhausted={isExhausted ? "true" : undefined}
      className={cn(
        "hidden h-8 shrink-0 items-center gap-sm rounded-md border px-md no-underline sm:inline-flex",
        "transition-colors duration-snap ease-snap focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        isExhausted
          ? "border-destructive/40 bg-destructive/10 hover:bg-destructive/20"
          : "border-info/40 bg-info/10 hover:bg-info/20",
      )}
    >
      <span className="font-sans text-label text-muted-foreground">
        {BALANCE_LABEL}
      </span>
      <span
        className={cn(
          "font-mono text-body-sm font-semibold tabular-nums",
          isExhausted ? "text-destructive" : "text-info",
        )}
      >
        {formatHeaderBalance(balanceNanos)}
      </span>
    </Link>
  );
}

export default BalanceLink;
