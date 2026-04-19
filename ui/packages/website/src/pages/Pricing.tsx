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
    ctaSource: "pricing_hobby_start_free",
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
    availability: "Upgrade when ready",
    price: "From Mission Control",
    audience: "For teams moving from trial usage to governed production delivery.",
    featured: true,
    ctaSource: "pricing_scale_upgrade",
    ctaLabel: "Upgrade in app",
    priceNote: "Operator-visible upgrade path after free credit exhaustion",
    proof: "Built for teams running automation across shared repos with stronger governance, quality visibility, and a direct Scale activation path.",
    highlights: [
      "Everything in Hobby, plus team workspaces and shared run history",
      "Longer execution windows and higher concurrency for active repos",
      "Sandbox resource governance with memory, CPU, and disk caps",
      "Agent run scoring with tier history and workspace baselines",
      "Failure analysis with deterministic context injection",
      "Upgrade from the app with a workspace subscription id",
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
