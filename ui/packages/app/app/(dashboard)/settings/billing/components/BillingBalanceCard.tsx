import {
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
            <span className={isExhausted ? "text-destructive" : undefined}>
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
          <p
            role="alert"
            className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive"
          >
            Balance exhausted. New zombie events are gate-blocked until a
            top-up — Stripe purchase ships in v2.1; contact{" "}
            <a href="mailto:support@usezombie.com" className="underline">
              support
            </a>{" "}
            for a manual top-up.
          </p>
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
          <span className="cursor-not-allowed">
            {/* Wrapper span captures hover for the Tooltip while the
                disabled button itself swallows pointer events. */}
            <Button variant="outline" disabled aria-disabled>
              Purchase Credits
            </Button>
          </span>
        </TooltipTrigger>
        <TooltipContent>{PURCHASE_TOOLTIP}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
