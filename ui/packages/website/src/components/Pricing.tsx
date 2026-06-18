import {
  Button,
  Card,
  DisplayLG,
  List,
  ListItem,
  SectionLabel,
} from "@agentsfleet/design-system";
import { WAITLIST_URL } from "../config";
import { trackSignupStarted } from "../analytics/posthog";
import { SUPPORT_EMAIL } from "../lib/contact";
import { PRICING_COPY, PRICING_PLANS, type PricingPlan } from "../lib/marketing-copy";
import { RATES_DISPLAY } from "../lib/rates";

const PRICING_TRACKING_SOURCE_PREFIX = "pricing_";
const ENTERPRISE_PLAN_ID = "enterprise";
const TRIAL_PLAN_ID = "trial";
const USAGE_PLAN_ID = "usage";

/*
 * Pricing — one simple story: free during the trial, then a single
 * usage-based run rate billed by the second only while an agent is actually
 * working (same rate on the platform or your own provider key), and the model
 * bill is always yours. No struck-through gradient, no staged billing grid,
 * no tier-extras list — those buried the "it's free right now" headline. Rate
 * Rate values come from RATES_DISPLAY (lib/rates.ts), the changelog-pinned single
 * source; this component only arranges them.
 */
export default function Pricing() {
  return (
    <section id="pricing" className="site-section" data-testid="pricing-block">
      <div className="wrap flex flex-col items-center gap-6 text-center">
        <SectionLabel className="mb-0">pricing</SectionLabel>
        <p
          data-testid="pricing-free-trial-banner"
          className="font-mono text-label uppercase tracking-label text-text-muted border border-border-strong rounded-sm px-md py-sm m-0"
        >
          {RATES_DISPLAY.FREE_TRIAL_PILL} — {PRICING_COPY.trialSuffix}
        </p>
        <DisplayLG>
          {PRICING_COPY.headline}
        </DisplayLG>
        <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-measure">
          {PRICING_COPY.lede}
        </p>

        <div className="grid w-full max-w-content grid-cols-1 gap-4 text-left lg:grid-cols-3">
          {PRICING_PLANS.map((plan) => (
            <PricingPlanCard key={plan.id} plan={plan} />
          ))}
        </div>

        <p className="font-sans text-body-sm leading-body-sm text-text-subtle m-0 max-w-measure">
          {PRICING_COPY.note}
        </p>
      </div>
    </section>
  );
}

function PricingPlanCard({ plan }: { plan: PricingPlan }) {
  // Pre-launch: both the free-trial ("Start free") and usage ("Get early
  // access") CTAs route to the waitlist; only Enterprise stays a contact mailto.
  const ctaHref =
    plan.id === ENTERPRISE_PLAN_ID
      ? `mailto:${SUPPORT_EMAIL}?subject=Enterprise%20agentsfleet`
      : WAITLIST_URL;
  // The waitlist is an external (Clerk) host — open it in a new tab to match
  // every other external link in the app and keep the marketing page alive.
  // The Enterprise mailto stays same-tab (a new tab for a mailto is pointless).
  const ctaExternal = ctaHref === WAITLIST_URL;
  const badge = "badge" in plan ? plan.badge : undefined;
  const suffix = "suffix" in plan ? plan.suffix : undefined;
  const testId = `pricing-card-${plan.id}`;

  return (
    <Card
      featured={plan.featured}
      badgeLabel={badge}
      data-testid={testId}
      className="flex h-full flex-col gap-5"
    >
      <div className="flex items-center gap-3">
        <h3 className="font-mono text-label uppercase tracking-label text-text-muted m-0">
          {plan.name}
        </h3>
      </div>

      <div className="font-mono text-fluid-display-md leading-display-md tracking-display-md text-text tabular-nums">
        {plan.id === USAGE_PLAN_ID ? (
          <>
            <span data-testid="pricing-rate-run">{RATES_DISPLAY.RUN_RATE_PER_SEC}</span>
            <span className="text-body text-text-muted"> · </span>
            <span data-testid="pricing-rate-run-hourly">{RATES_DISPLAY.RUN_RATE_PER_HOUR}</span>
          </>
        ) : (
          <>
            {plan.price}
            {suffix ? (
              <span className="text-body text-text-muted"> / {suffix}</span>
            ) : null}
          </>
        )}
      </div>

      {plan.id === USAGE_PLAN_ID ? (
        <p className="font-sans text-body-sm leading-body-sm text-text-muted m-0">
          Events are{" "}
          <span data-testid="pricing-rate-event" className="text-text">
            {RATES_DISPLAY.EVENT_RATE}
          </span>
          . Usage is metered only while running.
        </p>
      ) : null}

      {plan.id === TRIAL_PLAN_ID ? (
        <p className="font-sans text-body-sm leading-body-sm text-text-muted m-0">
          Includes {RATES_DISPLAY.STARTER_CREDIT} starter credit after the trial.
        </p>
      ) : null}

      <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
        {plan.features.map((feature) => (
          <ListItem key={feature} bullet="arrow" className="font-sans text-body-sm text-text-muted">
            {feature}
          </ListItem>
        ))}
      </List>

      <Button
        asChild
        variant="secondary"
        className="mt-auto min-h-11 w-full justify-center"
      >
        <a
          href={ctaHref}
          {...(ctaExternal ? { target: "_blank", rel: "noopener noreferrer" } : {})}
          data-testid={`pricing-cta-${plan.id}`}
          onClick={() =>
            trackSignupStarted({
              source: `${PRICING_TRACKING_SOURCE_PREFIX}${plan.id}`,
              surface: "pricing",
              mode: "humans",
            })
          }
        >
          {plan.cta}
        </a>
      </Button>

      {plan.id === ENTERPRISE_PLAN_ID ? (
        <p
          className="font-sans text-body-sm leading-body-sm text-text-muted m-0 text-center"
          data-testid="pricing-enterprise-email"
        >
          or email{" "}
          <a
            href={`mailto:${SUPPORT_EMAIL}`}
            className="text-text hover:underline"
            onClick={() =>
              trackSignupStarted({
                source: `${PRICING_TRACKING_SOURCE_PREFIX}${ENTERPRISE_PLAN_ID}_email`,
                surface: "pricing",
                mode: "humans",
              })
            }
          >
            {SUPPORT_EMAIL}
          </a>
        </p>
      ) : null}
    </Card>
  );
}
