import Link from "next/link";
import { cn } from "@agentsfleet/design-system";
import { formatBalance } from "@/lib/money";
import { NANOS_PER_USD } from "@/lib/types";

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
  const balance = formatHeaderBalance(balanceNanos);
  return (
    <Link
      href={BALANCE_HREF}
      aria-label={`${BALANCE_LABEL} ${balance}: ${isExhausted ? BALANCE_EXHAUSTED_ARIA_LABEL : BALANCE_ARIA_LABEL}`}
      data-exhausted={isExhausted ? "true" : undefined}
      className={cn(
        "flex min-h-9 w-full items-center justify-between gap-sm rounded-md px-md no-underline hover:bg-muted",
        "transition-colors duration-snap ease-snap focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
      )}
    >
      <span className="font-sans text-label text-muted-foreground">
        {BALANCE_LABEL}
      </span>
      <span
        className={cn(
          "font-mono text-body-sm font-semibold tabular-nums",
          isExhausted ? "text-destructive" : "text-foreground",
        )}
      >
        {balance}
      </span>
    </Link>
  );
}

export default BalanceLink;
