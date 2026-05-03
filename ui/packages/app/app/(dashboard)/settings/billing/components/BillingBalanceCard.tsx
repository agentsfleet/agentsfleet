import {
  Alert,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@usezombie/design-system";
import type { TenantBilling } from "@/lib/types";
import { formatDollars } from "../lib/groupCharges";

const PURCHASE_TOOLTIP = "Coming in v2.1 — contact support for a top-up.";

export type BillingBalanceCardProps = {
  billing: TenantBilling;
};

/**
 * Top-of-page balance card with a disabled Purchase Credits button. The
 * button is rendered grey via Tooltip + Button composition (no new
 * primitive needed per spec §8 / §9). When the tenant balance is
 * exhausted, the headline switches to a destructive treatment so the
 * "out of credits" state is visually unmistakable.
 */
export default function BillingBalanceCard({ billing }: BillingBalanceCardProps) {
  const isExhausted = billing.is_exhausted;
  const balance = billing.balance_cents ?? 0;

  return (
    <Card className="max-w-2xl animate-in fade-in-0 slide-in-from-top-1 duration-300">
      <CardHeader className="flex flex-row items-start justify-between gap-4">
        <div>
          <CardTitle className="text-3xl tabular-nums">
            <span
              data-exhausted={isExhausted}
              className={isExhausted ? "text-destructive" : undefined}
              data-testid="balance-headline"
            >
              {formatDollars(balance)} <span className="text-base font-normal text-muted-foreground">USD</span>
            </span>
          </CardTitle>
          <CardDescription className="mt-1">
            Covers all your zombie events.
          </CardDescription>
        </div>
        <PurchaseCreditsButton />
      </CardHeader>
      {isExhausted ? (
        <CardContent>
          <Alert variant="destructive" className="text-xs">
            Balance exhausted. New zombie events are gate-blocked until a
            top-up — Stripe purchase ships in v2.1; contact{" "}
            <a href="mailto:support@usezombie.com" className="underline">
              support
            </a>{" "}
            for a manual top-up.
          </Alert>
        </CardContent>
      ) : null}
    </Card>
  );
}

function PurchaseCreditsButton() {
  return (
    <TooltipProvider delayDuration={150}>
      <Tooltip>
        <TooltipTrigger asChild>
          {/* Disabled <button> doesn't fire pointer events, so the
              tooltip won't show without a wrapper. tabIndex={0} keeps
              the trigger keyboard-reachable; cursor-not-allowed lives
              on the wrapper because the disabled button blocks its
              own pointer events. */}
          <span
            tabIndex={0}
            aria-describedby="purchase-credits-tooltip"
            className="inline-block cursor-not-allowed rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            data-testid="purchase-credits-trigger"
          >
            <Button
              variant="outline"
              disabled
              aria-disabled
              tabIndex={-1}
              className="pointer-events-none"
            >
              Purchase Credits
            </Button>
          </span>
        </TooltipTrigger>
        <TooltipContent id="purchase-credits-tooltip">{PURCHASE_TOOLTIP}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
