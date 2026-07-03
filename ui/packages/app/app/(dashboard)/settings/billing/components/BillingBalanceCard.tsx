import {
  Alert,
  Button,
  Card,
  CardContent,
  EYEBROW_CLASS,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agentsfleet/design-system";
import { cn } from "@/lib/utils";
import type { TenantBilling } from "@/lib/types";
import { SUPPORT_EMAIL } from "@/lib/contact";
import { formatDollars, type ChargeSummary } from "../lib/charges";

const PURCHASE_TOOLTIP = "Contact support to top up your balance.";

export type BillingBalanceCardProps = {
  billing: TenantBilling;
  summary: ChargeSummary;
};

/**
 * Top-of-page balance card: amount + a full-width consumption meter (fills on
 * load) + a caption that rides the meter's end, with the Purchase CTA pinned
 * top-right of the head row (no stranded gap). When the balance is exhausted
 * the headline switches to a destructive treatment and an alert appears.
 */
export default function BillingBalanceCard({ billing, summary }: BillingBalanceCardProps) {
  const isExhausted = billing.is_exhausted;

  return (
    <Card>
      <CardContent className="space-y-4 p-6">
        <div className="flex flex-row items-end justify-between gap-4">
          <div>
            <div className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
              Balance
            </div>
            <div className="mt-1 text-display-md font-semibold leading-display-md tracking-normal tabular-nums">
              <span
                data-exhausted={isExhausted}
                className={isExhausted ? "text-destructive" : undefined}
                data-testid="balance-headline"
              >
                {formatDollars(billing.balance_nanos)}
              </span>
              <span className="ml-1.5 font-mono text-base font-normal text-muted-foreground">
                USD
              </span>
            </div>
          </div>
          <PurchaseCreditsButton />
        </div>

        <div className="app-meter" data-testid="balance-meter" aria-hidden="true">
          <span style={{ width: `${summary.meterPct}%` }} />
        </div>

        <div className="flex items-center justify-between gap-4 text-sm text-muted-foreground">
          <div>
            Covers all Fleet events · <span className="font-medium text-foreground">pay as you go</span>
          </div>
          <div className="font-mono text-xs" data-testid="balance-usage">
            spent <span className="text-foreground">{formatDollars(summary.spentNanos)}</span> ·{" "}
            <span className="text-foreground">{summary.eventCount}</span>{" "}
            {summary.eventCount === 1 ? "event" : "events"}
          </div>
        </div>

        {isExhausted ? (
          <Alert variant="destructive" className="text-xs">
            Balance exhausted. New Fleet events are gate-blocked until you top up — contact{" "}
            <a href={`mailto:${SUPPORT_EMAIL}`} className="underline">
              support
            </a>{" "}
            for a top-up.
          </Alert>
        ) : null}
      </CardContent>
    </Card>
  );
}

function PurchaseCreditsButton() {
  return (
    <TooltipProvider delayDuration={150}>
      <Tooltip>
        <TooltipTrigger asChild>
          {/* Disabled-button-with-tooltip a11y workaround: a disabled <Button>
           * can't receive focus, so a non-interactive span wrapper carries the
           * focus + describedby. jsx-a11y/no-noninteractive-tabindex flags this;
           * the wrapper is the recommended ARIA pattern for keyboard-reachable
           * disabled affordances. */}
          <span
            // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex
            tabIndex={0}
            aria-describedby="purchase-credits-tooltip"
            className="inline-block cursor-not-allowed rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            data-testid="purchase-credits-trigger"
          >
            <Button variant="outline" disabled aria-disabled tabIndex={-1} className="pointer-events-none">
              Purchase credits
            </Button>
          </span>
        </TooltipTrigger>
        <TooltipContent id="purchase-credits-tooltip">{PURCHASE_TOOLTIP}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
