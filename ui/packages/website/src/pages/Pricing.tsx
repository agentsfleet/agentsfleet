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
  leadIntent?: LeadIntent;
  highlights: string[];
};

const roadmapSignals = [
  "BYOK/BYOM and direct provider billing",
  "Validated PR flow before human review",
  "Upcoming Firecracker resource governance",
  "Upcoming agent scoring, failure analysis, and learning loops",
];

const tiers: Tier[] = [
  {
    name: "Hobby",
    availability: "Available now",
    price: "Free",
    audience: "For solo builders and early evaluation.",
    isLive: true,
    ctaLabel: "Start free",
    highlights: [
      "$10 credit included with no expiry",
      "1 workspace and automated pull request generation",
      "Harness validation in the PR flow",
      "BYOK/BYOM with no token markup",
    ],
  },
  {
    name: "Core",
    availability: "Launching next",
    price: "Notify me",
    audience: "For teams moving from trial usage to production workflow.",
    featured: true,
    ctaLabel: "Notify me",
    leadIntent: {
      ctaId: "pricing_core_notify",
      planInterest: "Core",
      title: "Get notified when Core opens",
      description:
        "Core is the first paid team tier: multi-workspace rollout, saved agent configuration, longer run history, and higher runtime limits.",
      actionLabel: "Notify me",
    },
    highlights: [
      "Everything in Hobby, plus team-level workspaces",
      "Dynamic agent profiles with shared run history",
      "Longer execution windows and higher concurrency",
      "Priority rollout access for production teams",
    ],
  },
  {
    name: "Pro",
    availability: "Upcoming",
    price: "Notify me",
    audience: "For platform teams that need stronger control and richer run intelligence.",
    ctaLabel: "Notify me",
    leadIntent: {
      ctaId: "pricing_pro_notify",
      planInterest: "Pro",
      title: "Get notified when Pro opens",
      description:
        "Pro is where the upcoming M4 and M9 capabilities land first: Firecracker-backed resource controls, score history, failure analysis, and next-run context injection.",
      actionLabel: "Notify me",
    },
    highlights: [
      "Sandbox resource governance with memory, CPU, and disk caps",
      "Agent run scoring with tier history and workspace baselines",
      "Failure analysis with deterministic context injection",
      "Higher limits for larger repos and busier teams",
    ],
  },
  {
    name: "Enterprise",
    availability: "Design partner",
    price: "Talk to sales",
    audience: "For regulated or large-scale teams shaping the rollout with us.",
    ctaLabel: "Talk to sales",
    leadIntent: {
      ctaId: "pricing_enterprise_sales",
      planInterest: "Enterprise",
      title: "Talk to sales about Enterprise",
      description:
        "Enterprise expands the upcoming governance path with stricter egress policy, override controls, rollout support, and operator-facing approval workflows.",
      actionLabel: "Request follow-up",
    },
    highlights: [
      "Per-workspace resource override policy",
      "Network egress governance and audit-oriented controls",
      "Approval and veto workflows for harness improvement",
      "Dedicated rollout support and enterprise onboarding",
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
          <p className="eyebrow">Launch posture</p>
          <h2>Only pricing captures demand.</h2>
        </div>
        <p>
          The homepage stays focused on product understanding. `/pricing` is where upcoming paid
          demand is collected, with cleaner attribution and less friction for humans.
        </p>
      </div>

      <FAQ />
    </section>
  );
}
