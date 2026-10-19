import Link from "next/link";
import { cn } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { NANOS_PER_USD } from "@/lib/types";

// Header balance links to Billing. Information blue separates the amount
// from navigation; exhaustion keeps the destructive treatment.
export const BALANCE_HREF = "/settings/billing";
export const BALANCE_LABEL = "Credits";
export const BALANCE_ARIA_LABEL = "Credit balance";
export const BALANCE_EXHAUSTED_ARIA_LABEL = "Credit balance exhausted";

const CENTS_PER_USD = 100n;
const NANOS_PER_CENT = BigInt(NANOS_PER_USD) / CENTS_PER_USD;
const CENT_DIGITS = 2;

/** Read cents at a glance; retain smaller credits and debts below a cent. */
export function formatHeaderBalance(nanos: string): string {
  const amount = BigInt(nanos);
  const magnitude = amount < 0n ? -amount : amount;
  return magnitude < NANOS_PER_CENT ? formatBalance(nanos) : formatBalance(nanos, CENT_DIGITS);
}

export function BalanceLink({
  balanceNanos,
  isExhausted,
}: {
  balanceNanos: string;
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
