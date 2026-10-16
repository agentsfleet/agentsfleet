import { Button, Card, DisplayLG, Section, SectionLabel } from "@agentsfleet/design-system";
import { APP_SIGN_UP_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
import { SUPPORT_EMAIL } from "../lib/contact";
import { HERO_PRIMARY_LABEL, PRICING_COPY } from "../lib/marketing-copy";

export default function Pricing() {
  return (
    <Section asChild className="site-section" data-testid="pricing-block">
      <section id="pricing" aria-label="Getting started and pricing">
        <div className="wrap early-access-layout">
          <div className="flex flex-col items-start gap-5">
            <SectionLabel as="p" className="mb-0">Build with us</SectionLabel>
            <DisplayLG>{PRICING_COPY.headline}</DisplayLG>
            <p className="text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">{PRICING_COPY.lede}</p>
            <CreateFleetAction />
            <a href={`mailto:${SUPPORT_EMAIL}`}
              onClick={() => trackNavigationClicked({ source: "pricing_contact", surface: "pricing", target: "email" })}
              className="inline-flex min-h-11 items-center text-body-sm text-text-muted underline underline-offset-4">
              Tell us about your workflow
            </a>
          </div>
          <Card className="flex flex-col gap-5">
            <p data-testid="pricing-early-access-banner" className="m-0 text-body-sm font-medium text-pulse ph-no-capture">{PRICING_COPY.status}</p>
            <h3 className="m-0 font-sans text-heading font-medium">What to expect on cost</h3>
            <p className="m-0 text-body-sm text-text-muted">{PRICING_COPY.runtime}</p>
            <p className="m-0 text-body-sm text-text-muted">{PRICING_COPY.models}</p>
            <p className="m-0 border-t border-border pt-4 text-body-sm text-text">{PRICING_COPY.note}</p>
          </Card>
        </div>
      </section>
    </Section>
  );
}

function CreateFleetAction() {
  return (
    <Button wrap asChild className="min-h-11" data-testid="pricing-cta-early-access">
      <a href={APP_SIGN_UP_URL} target="_blank" rel="noopener noreferrer"
        onClick={() => trackSignupStarted({ source: "pricing_early_access", surface: "pricing", mode: "humans" })}
      >{HERO_PRIMARY_LABEL} <span aria-hidden="true">→</span></a>
    </Button>
  );
}
