import type { ReactNode } from "react";
import { Button, Card, SectionLabel } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupStarted } from "../analytics/posthog";
import { SUPPORT_EMAIL } from "../lib/contact";
import { RATES_DISPLAY } from "../lib/rates";

/*
 * Pricing — one simple story: free during the trial, then one honest
 * forward rate per reasoning stage (10× cheaper on your own provider key),
 * and the model bill is always yours. No struck-through gradient, no
 * per-stage billing grid, no tier-extras list — those buried the "it's
 * free right now" headline. Rate VALUES come from RATES_DISPLAY
 * (lib/rates.ts), the changelog-pinned single source; this component only
 * arranges them.
 */
export default function Pricing() {
  return (
    <section id="pricing" className="site-section" data-testid="pricing-block">
      <div className="wrap flex flex-col gap-6">
        <SectionLabel className="mb-0">pricing</SectionLabel>

        <Card data-testid="pricing-rate-card" className="flex flex-col gap-5">
          <p
            data-testid="pricing-free-trial-banner"
            className="font-mono text-fluid-display-md leading-display-md tracking-display-md font-medium text-text m-0 max-w-narrow"
          >
            {RATES_DISPLAY.FREE_TRIAL_BANNER}
          </p>

          <RateTable />

          <p className="font-sans text-body leading-body text-text-muted m-0 max-w-narrow">
            A <span className="text-text">stage</span> is one reasoning step. Most diagnoses
            resolve in 1–5 stages.
          </p>

          <p
            data-testid="pricing-design-partner-note"
            className="font-sans text-body-sm leading-body-sm text-pulse m-0 max-w-narrow"
          >
            Want a hand calibrating a zombie or to join as a design partner? Email{" "}
            <a
              href={`mailto:${SUPPORT_EMAIL}?subject=Design%20partner`}
              className="underline hover:text-text"
            >
              {SUPPORT_EMAIL}
            </a>
            .
          </p>

          <Button asChild className="self-start">
            <a
              href={APP_BASE_URL}
              data-testid="pricing-install-cta"
              onClick={() =>
                trackSignupStarted({
                  source: "pricing_install",
                  surface: "pricing",
                  mode: "humans",
                })
              }
            >
              → get early access
            </a>
          </Button>
        </Card>
      </div>
    </section>
  );
}

function RateTable() {
  return (
    <dl data-testid="pricing-rate-table" className="m-0 flex flex-col gap-3">
      <RateRow label="Event receipt">
        <span data-testid="pricing-rate-event" className="text-text">
          {RATES_DISPLAY.EVENT_RATE}
        </span>
        <span className="text-text-muted">— always</span>
      </RateRow>
      <RateRow label="Reasoning stage">
        <span data-testid="pricing-rate-stage-platform" className="text-text tabular-nums">
          {RATES_DISPLAY.STAGE_PLATFORM}
        </span>
        <span className="text-text-muted">platform</span>
        <span className="text-text-subtle" aria-hidden="true">
          ·
        </span>
        <span data-testid="pricing-rate-stage-self-managed" className="text-text tabular-nums">
          {RATES_DISPLAY.STAGE_SELF_MANAGED}
        </span>
        <span className="text-text-muted">on your own key</span>
      </RateRow>
      <RateRow label="Model tokens">
        <span className="text-text">your provider</span>
        <span className="text-text-muted">— your bill, we mark up zero</span>
      </RateRow>
    </dl>
  );
}

function RateRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 border-b border-border pb-3 last:border-0 last:pb-0">
      <dt className="font-mono text-body-sm text-text-muted">{label}</dt>
      <dd className="m-0 flex flex-wrap items-baseline gap-x-2 font-mono text-body">
        {children}
      </dd>
    </div>
  );
}
