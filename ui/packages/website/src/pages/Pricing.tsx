import FAQ from "../components/FAQ";
import { Button } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupCompleted, trackLeadCaptureClicked } from "../analytics/posthog";
import { MODE_HUMANS } from "../constants/mode";

type Tier = {
  name: string;
  availability: string;
  price: string;
  audience: string;
  featured?: boolean;
  ctaSource: string;
  ctaLabel: string;
  priceNote?: string;
  proof: string;
  highlights: string[];
};

const roadmapSignals = [
  "BYOK with no token markup",
  "Open source runtime",
  "Three triggers, one reasoning loop",
  "Self-host arrives in v3",
];

const tiers: Tier[] = [
  {
    name: "Hobby",
    availability: "Available now",
    price: "Free",
    audience: "For solo operators evaluating the wedge.",
    ctaSource: "pricing_hobby_start_free",
    ctaLabel: "Start free",
    proof: "Best for installing platform-ops on a real repo and seeing a real diagnosis.",
    highlights: [
      "$5 starter credit, never expires",
      "1 workspace",
      "BYOK on Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot",
      "Hosted execution metered on event receipt + per stage; no token markup",
    ],
  },
  {
    name: "Scale",
    availability: "Upgrade when ready",
    price: "From Mission Control",
    audience: "For teams running agents across shared workspaces.",
    featured: true,
    ctaSource: "pricing_scale_upgrade",
    ctaLabel: "Upgrade in app",
    priceNote: "Operator-visible upgrade path after free credit exhaustion",
    proof: "Built for teams operating multiple agents across shared workspaces with approval gates and budget controls.",
    highlights: [
      "Everything in Hobby",
      "Credit-pool billing, Amp-style: debits on event receipt + per-stage execution",
      "Multiple workspaces with shared event history",
      "Higher concurrency and longer per-stage windows",
      "Approval gating in dashboard and Slack DM",
      "Workspace-scoped credentials and webhooks",
      "Priority support",
    ],
  },
];

export default function Pricing() {
  return (
    <section className="stack route-fade pricing-page">
      <div className="pricing-hero">
        <p className="eyebrow">pricing</p>
        <h1>Start free. Upgrade when you need stronger control.</h1>
        <p className="lead">
          UseZombie sells durable execution and operational ownership — not marked-up model usage.
          Bring your own model key; pay your provider directly. Hosted execution is metered against
          a credit pool with a $5 starter grant — debits fire on event receipt and per-stage
          execution.
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
              {tier.priceNote ? <p className="pricing-card-note">{tier.priceNote}</p> : null}
            </div>
            <Button
              asChild
              variant={tier.featured ? "default" : "double-border"}
              className="pricing-card-cta"
            >
              <a
                href={APP_BASE_URL}
                onClick={() =>
                  tier.featured
                    ? trackLeadCaptureClicked({ page: "pricing", surface: "pricing_card", cta_id: tier.ctaSource, plan_interest: "Scale" })
                    : trackSignupCompleted({ source: tier.ctaSource, surface: "pricing", mode: MODE_HUMANS })
                }
              >
                {tier.ctaLabel}
              </a>
            </Button>

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

      <div className="pricing-bottom-band">
        <div>
          <p className="eyebrow">When to move up</p>
          <h2>Start on Hobby. Move to Scale when agents become shared infrastructure.</h2>
        </div>
        <p>
          Hobby validates the wedge on one repo. Scale adds shared event history, approval flows,
          and the budget controls a team needs once agents own real production outcomes.
        </p>
      </div>

      <FAQ />
    </section>
  );
}
