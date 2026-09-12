import Link from "next/link";
import { cn } from "@agentsfleet/design-system";
import {
  formatDollars,
  MIN_VISIBLE_NANOS,
  SUBVISIBLE_AMOUNT_LABEL,
} from "@/app/(dashboard)/settings/billing/lib/charges";

/*
 * What is left to spend, in the header, on every page.
 *
 * The balance used to live only on the billing page, so the answer to "can
 * this fleet still run?" was a navigation away. It is one figure, so it rides
 * the trailing cluster beside the workspace switcher rather than taking a
 * meter or a card: the fraction consumed is a billing-page question, the
 * number left is an everywhere question.
 *
 * Labelled and bounded, not a muted glyph and a number. A quiet coin icon
 * read as header chrome — the operator scanning for "how much is left"
 * skipped it — and simply growing the type made the figure loud without
 * making it findable, since nothing around it agreed to be a row of chips.
 *
 * So it is a chip: its own border and tinted ground in the mint a live fleet
 * wears, which is what credits buy. The bound is what the eye lands on, so
 * the type can stay at the header's own size. It takes the workspace
 * switcher's own height, because two bounded controls side by side at
 * different heights read as a mistake before they read as a hierarchy.
 * Exhausted, the whole chip turns destructive — that is the one state that
 * changes what an operator does next, because new fleet events gate-block
 * until a top-up.
 *
 * Server-rendered from the layout's own read, so it refreshes when a page
 * does. A tab left open overnight shows last night's figure; the billing page
 * is the live answer, and this links straight to it.
 */

export const BALANCE_HREF = "/settings/billing";
export const BALANCE_LABEL = "Credits";
export const BALANCE_ARIA_LABEL = "Credit balance";
export const BALANCE_EXHAUSTED_ARIA_LABEL = "Credit balance exhausted";

const NANOS_PER_CENT = 10_000_000;
const NANOS_PER_USD = NANOS_PER_CENT * 100;
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
          : "border-pulse/40 bg-pulse/10 hover:bg-pulse/20",
      )}
    >
      <span className="font-sans text-label uppercase tracking-label text-muted-foreground">
        {BALANCE_LABEL}
      </span>
      <span
        className={cn(
          // Header-sized, deliberately NOT the 13px `text-mono` step the data
          // surfaces use. This figure is chrome: it sits beside the workspace
          // switcher and the nav, and its size is set by those neighbours
          // rather than by the kind of value it holds. Growing or shrinking it
          // away from 14px made it loud without making it findable, which is
          // what the bounded chip does instead.
          "font-mono text-body-sm font-medium tabular-nums",
          isExhausted ? "text-destructive" : "text-pulse",
        )}
      >
        {formatHeaderBalance(balanceNanos)}
      </span>
    </Link>
  );
}

export default BalanceLink;
