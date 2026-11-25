import { useId } from "react";
import { Button, DisplayLG, Section, cn } from "@agentsfleet/design-system";
import { linkTracking } from "../analytics/link-tracking";
import { PRICING_COPY } from "../lib/marketing-copy";
import { SECTION_SPACING, SITE_CONTAINER } from "./site-classes";

export function Pricing({ waitlistUrl }: { waitlistUrl: string }) {
  const heading = useId();
  return (
    <Section id="pricing" data-testid="pricing-block" aria-labelledby={heading} variant="divided" className="scroll-mt-xl bg-card">
      <div className={cn(SITE_CONTAINER, SECTION_SPACING, "flex flex-wrap items-center justify-between gap-xl lg:gap-3xl")}>
        <div className="grid gap-md">
          <DisplayLG id={heading} className="font-title">Early access pricing</DisplayLG>
          <p className="max-w-prose text-conversation leading-conversation text-text-muted">
            {PRICING_COPY.runtime} {PRICING_COPY.note}
          </p>
        </div>
        <div className="flex flex-wrap gap-md">
          <Button asChild wrap className="min-h-11">
            <a
              href={waitlistUrl}
              target="_blank"
              rel="noopener noreferrer"
              data-testid="pricing-cta-early-access"
              {...linkTracking("pricing_signup", "pricing", "signup")}
            >
              Join waitlist
            </a>
          </Button>
          <Button asChild wrap variant="outline" className="min-h-11">
            <a href="/agents" {...linkTracking("pricing_terminal", "pricing", "agents")}>
              Use the terminal
            </a>
          </Button>
        </div>
      </div>
    </Section>
  );
}
