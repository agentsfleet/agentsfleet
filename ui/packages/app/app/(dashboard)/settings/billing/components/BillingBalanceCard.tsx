import {
  Alert,
  Button,
  Card,
  CardContent,
  cn,
  EYEBROW_CLASS,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  UsageBar,
} from "@agentsfleet/design-system";
import { CoinsIcon } from "lucide-react";
import type { TenantBilling } from "@/lib/types";
import { SUPPORT_EMAIL } from "@/lib/contact";
import { formatDollars, type ChargeSummary } from "../lib/charges";

const BUY_CREDITS_TOOLTIP = `Email ${SUPPORT_EMAIL} to top up your balance.`;

export type BillingBalanceCardProps = {
  billing: TenantBilling;
  summary: ChargeSummary;
};

/**
 * Top-of-page balance card: amount + a full-width consumption meter (fills on
 * load) + a caption that rides the meter's end, with the Buy credits CTA pinned
 * top-right of the head row (no stranded gap). When the balance is exhausted
 * the headline switches to a destructive treatment and an alert appears.
 */
export default function BillingBalanceCard({ billing, summary }: BillingBalanceCardProps) {
  const isExhausted = billing.is_exhausted;

  return (
    <Card>
      <CardContent className="space-y-3 p-4">
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
          <BuyCreditsButton />
        </div>

        <UsageBar
          data-testid="balance-meter"
          pct={summary.meterPct}
          sublabel={
            <div className="flex justify-end" data-testid="balance-usage">
              spent <span className="text-foreground">{formatDollars(summary.spentNanos)}</span>{" "}
              ·{" "}
              <span className="text-foreground">{summary.eventCount}</span>{" "}
              {summary.eventCount === 1 ? "event" : "events"}
            </div>
          }
        />

        {isExhausted ? (
          <Alert variant="destructive" className="text-xs">
            Balance exhausted. New fleet events are gate-blocked until you top up — contact{" "}
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

function BuyCreditsButton() {
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          {/* A real mailto link, not a disabled control — a control that acts
           * must not claim `disabled`/`aria-disabled` to assistive tech. Kept
           * visually muted (outline variant) since there's no in-app purchase
           * flow yet; the tooltip states exactly what clicking does. */}
          <Button variant="outline" asChild data-testid="buy-credits-trigger">
            <a href={`mailto:${SUPPORT_EMAIL}`} aria-describedby="buy-credits-tooltip">
              <CoinsIcon size={14} aria-hidden="true" />
              Buy credits
            </a>
          </Button>
        </TooltipTrigger>
        <TooltipContent id="buy-credits-tooltip">{BUY_CREDITS_TOOLTIP}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
