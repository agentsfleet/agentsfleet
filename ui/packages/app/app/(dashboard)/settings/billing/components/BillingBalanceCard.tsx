import {
  Alert, Button, DashboardPanel, DashboardPanelHeader, DashboardPanelDescription,
  SectionLabel, Tooltip, TooltipContent, TooltipTrigger,
} from "@agentsfleet/design-system";
import { CoinsIcon } from "lucide-react";
import type { TenantBilling } from "@/lib/types";
import { SUPPORT_EMAIL } from "@/lib/contact";
import { formatBalance } from "@/lib/money";

export type BillingBalanceCardProps = { billing: TenantBilling };

export default function BillingBalanceCard({ billing }: BillingBalanceCardProps) {
  return <DashboardPanel className="space-y-xl p-2xl">
    <DashboardPanelHeader className="gap-xl">
      <div className="space-y-md">
        <SectionLabel>Available credits</SectionLabel>
        <p className="flex items-baseline gap-md text-display-md font-semibold leading-display-md tabular-nums">
          <span data-exhausted={billing.is_exhausted} data-testid="balance-headline"
            className={billing.is_exhausted ? "text-destructive" : "text-foreground"}>{formatBalance(billing.balance_nanos)}</span>
          <span className="text-body font-normal text-muted-foreground">USD</span>
        </p>
        <DashboardPanelDescription>Credits cover your fleets’ usage across this account.</DashboardPanelDescription>
      </div>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button asChild data-testid="buy-credits-trigger">
            <a href={`mailto:${SUPPORT_EMAIL}`} aria-describedby="buy-credits-tooltip"><CoinsIcon size={16} aria-hidden="true" />Buy credits</a>
          </Button>
        </TooltipTrigger>
        <TooltipContent id="buy-credits-tooltip">Email {SUPPORT_EMAIL} to top up your balance.</TooltipContent>
      </Tooltip>
    </DashboardPanelHeader>
    {billing.is_exhausted ? <Alert variant="warning">
      Balance exhausted. Top up to let your fleets process new events.
    </Alert> : null}
    <DashboardPanelDescription className="border-t border-border pt-lg">
      Need a top-up, an invoice, or billing help? <a href={`mailto:${SUPPORT_EMAIL}`} className="text-foreground underline underline-offset-4">Contact support</a>.
    </DashboardPanelDescription>
  </DashboardPanel>;
}
