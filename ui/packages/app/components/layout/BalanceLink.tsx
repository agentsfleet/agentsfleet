import Link from "next/link";
import { CoinsIcon } from "lucide-react";
import { cn } from "@agentsfleet/design-system";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";

/*
 * What is left to spend, in the header, on every page.
 *
 * The balance used to live only on the billing page, so the answer to "can
 * this fleet still run?" was a navigation away. It is one figure, so it rides
 * the trailing cluster beside the workspace switcher rather than taking a
 * meter or a card: the fraction consumed is a billing-page question, the
 * number left is an everywhere question.
 *
 * Exhausted, it turns destructive — that is the one state that changes what
 * an operator does next, because new fleet events gate-block until a top-up.
 *
 * Server-rendered from the layout's own read, so it refreshes when a page
 * does. A tab left open overnight shows last night's figure; the billing page
 * is the live answer, and this links straight to it.
 */

export const BALANCE_HREF = "/settings/billing";
export const BALANCE_LABEL = "Credit balance";
export const BALANCE_EXHAUSTED_LABEL = "Credit balance exhausted";
const ICON_SIZE = 13;

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
      aria-label={isExhausted ? BALANCE_EXHAUSTED_LABEL : BALANCE_LABEL}
      title={isExhausted ? BALANCE_EXHAUSTED_LABEL : BALANCE_LABEL}
      data-exhausted={isExhausted ? "true" : undefined}
      className={cn(
        "hidden shrink-0 items-center gap-sm rounded-md px-sm py-xs font-mono text-label tabular-nums no-underline sm:inline-flex",
        "transition-colors duration-snap ease-snap hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        isExhausted ? "text-destructive" : "text-muted-foreground hover:text-foreground",
      )}
    >
      <CoinsIcon size={ICON_SIZE} aria-hidden="true" />
      {formatDollars(balanceNanos)}
    </Link>
  );
}

export default BalanceLink;
