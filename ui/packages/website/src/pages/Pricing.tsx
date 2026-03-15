import { useState } from "react";
import FAQ from "../components/FAQ";
import PricingLeadCapture from "../components/PricingLeadCapture";
import { Button } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import {
  trackLeadCaptureClicked,
  trackLeadCaptureOpened,
  trackSignupCompleted,
} from "../analytics/posthog";
import { MODE_HUMANS } from "../constants/mode";

type LeadIntent = {
  ctaId: string;
  planInterest: string;
  title: string;
  description: string;
  actionLabel: string;
};

type Tier = {
  name: string;
  availability: string;
  price: string;
  audience: string;
  featured?: boolean;
  isLive?: boolean;
  ctaLabel: string;
  priceNote?: string;
  proof: string;
  leadIntent?: LeadIntent;
  highlights: string[];
};

const roadmapSignals = [
  "Direct provider billing with BYOK/BYOM",
  "Validated PR delivery before review",
  "Run quality scoring and failure analysis",
  "Sandbox governance and team controls",
];

const tiers: Tier[] = [
  {
    name: "Hobby",
    availability: "Available now",
    price: "Free",
    audience: "For solo builders and early evaluation.",
    isLive: true,
    ctaLabel: "Start free",
    proof: "Best for getting real PRs running without a credit card.",
    highlights: [
      "$10 credit included with no expiry",
      "1 workspace and automated pull request generation",
      "Harness validation in the PR flow",
      "BYOK/BYOM with no token markup",
    ],
  },
  {
    name: "Scale",
    availability: "Waitlist open",
    price: "Notify me",
    audience: "For teams moving from trial usage to governed production delivery.",
    featured: true,
    ctaLabel: "Notify me",
    priceNote: "Join the waitlist for rollout pricing",
    proof: "Built for teams running automation across shared repos with stronger governance and quality visibility.",
    leadIntent: {
      ctaId: "pricing_scale_notify",
      planInterest: "Scale",
      title: "Get notified when Scale opens",
      description:
        "Scale adds shared workspaces, sandbox governance, score history, failure analysis, and controlled improvement workflows for teams.",
      actionLabel: "Notify me",
    },
    highlights: [
      "Everything in Hobby, plus team workspaces and shared run history",
      "Longer execution windows and higher concurrency for active repos",
      "Sandbox resource governance with memory, CPU, and disk caps",
      "Agent run scoring with tier history and workspace baselines",
      "Failure analysis with deterministic context injection",
      "Priority access to the rollout as the paid plan opens",
    ],
  },
];

export default function Pricing() {
  const [activeLeadIntent, setActiveLeadIntent] = useState<LeadIntent | null>(null);

  function openLeadIntent(intent: LeadIntent) {
    setActiveLeadIntent(intent);
    trackLeadCaptureClicked({
      page: "pricing",
      surface: "pricing_card",
      cta_id: intent.ctaId,
      plan_interest: intent.planInterest,
    });
    trackLeadCaptureOpened({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: intent.ctaId,
      plan_interest: intent.planInterest,
    });
  }

  return (
    <section className="stack route-fade pricing-page">
      <div className="pricing-hero">
        <p className="eyebrow">pricing</p>
        <h1>Start free. Upgrade when you need stronger control.</h1>
        <p className="lead">
          UseZombie sells agent execution and delivery control, not marked-up model usage.
          Paid plans are where deeper sandbox governance, agent scoring, and rollout controls
          come online.
        </p>

        <div className="pricing-roadmap-strip" aria-label="Pricing proof points">
          {roadmapSignals.map((signal) => (
            <span key={signal}>{signal}</span>
          ))}
        </div>
      </div>

      <div className="pricing-grid">
        {tiers.map((tier) => (
          <article key={tier.name} className={`pricing-card${tier.featured ? " pricing-card--featured" : ""}`}>
            <div className="pricing-card-head">
              <span className="pricing-card-badge">{tier.availability}</span>
              <h2>{tier.name}</h2>
              <p className="pricing-card-audience">{tier.audience}</p>
              <p className="pricing-card-price">{tier.price}</p>
            </div>

            {tier.isLive ? (
                <Button
                  to={APP_BASE_URL}
                  className="pricing-card-cta"
                  onClick={() => trackSignupCompleted({ source: "pricing_hobby_start_free", surface: "pricing", mode: MODE_HUMANS })}
                >
                {tier.ctaLabel}
              </Button>
            ) : (
              <Button
                variant={tier.featured ? "primary" : "double-border"}
                className="pricing-card-cta"
                onClick={() => tier.leadIntent && openLeadIntent(tier.leadIntent)}
              >
                {tier.ctaLabel}
              </Button>
            )}

            <div className="pricing-card-proof">
              <span className="pricing-card-proof-label">Why this plan</span>
              <p>{tier.proof}</p>
            </div>

            <ul className="pricing-card-points">
              {tier.highlights.map((point) => (
                <li key={point}>{point}</li>
              ))}
            </ul>
          </article>
        ))}
      </div>

      <PricingLeadCapture intent={activeLeadIntent} />

      <div className="pricing-bottom-band">
        <div>
          <p className="eyebrow">When to move up</p>
          <h2>Start on Hobby. Move to Scale when automation becomes shared infrastructure.</h2>
        </div>
        <p>
          Hobby is enough to validate the workflow on real repos. Scale adds shared history,
          stronger controls, and deeper quality analysis for teams operating across multiple repos.
        </p>
      </div>

      <FAQ />
    </section>
  );
}
