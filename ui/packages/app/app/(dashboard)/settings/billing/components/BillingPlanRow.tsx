import { ActivityIcon } from "lucide-react";
import { Badge, Card, CardContent } from "@agentsfleet/design-system";
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
        {/* Tight text stack (no Button chrome) so the two lines right-align and
            baseline-match the left column instead of riding low on the h-8
            control box. Inline pulse-link idiom matches the Models page. */}
        <div className="flex flex-col items-start gap-1 sm:items-end">
          <div className="text-xs text-muted-foreground">Running heavy volume?</div>
          <a
            href={`mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(VOLUME_SUBJECT)}`}
            className="text-body-sm font-medium text-pulse underline-offset-2 hover:underline focus-visible:underline"
          >
            Talk to us about volume pricing →
          </a>
        </div>
      </CardContent>
    </Card>
  );
}
