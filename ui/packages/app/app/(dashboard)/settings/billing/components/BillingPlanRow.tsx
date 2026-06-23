import { ActivityIcon } from "lucide-react";
import { Badge, Button, Card, CardContent } from "@agentsfleet/design-system";
import { SUPPORT_EMAIL } from "@/lib/contact";

const VOLUME_SUBJECT = "Volume pricing enquiry";

/**
 * Honest single-row "plan" — consumption billing has no seats, so there is no
 * plan grid to choose from. One "Pay as you go" row marked Current, plus a
 * volume-pricing contact link for heavy usage.
 */
export default function BillingPlanRow() {
  return (
    <Card data-testid="billing-plan-row">
      <CardContent className="flex flex-col gap-4 p-6 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-start gap-3">
          <ActivityIcon size={18} className="mt-0.5 shrink-0 text-pulse" aria-hidden="true" />
          <div>
            <div className="flex items-center gap-2 font-medium">
              Pay as you go
              <Badge variant="default">Current</Badge>
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Billed per Fleet event from your balance. Nothing to choose — top up when you&rsquo;re low.
            </p>
          </div>
        </div>
        <div className="sm:text-right">
          <div className="text-xs text-muted-foreground">Running heavy volume?</div>
          <Button variant="link" size="sm" asChild className="px-0">
            <a href={`mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(VOLUME_SUBJECT)}`}>
              Talk to us about volume pricing →
            </a>
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
