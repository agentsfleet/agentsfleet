import FAQ from "../components/FAQ";
import { Button } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL, MAILTO_SCALE_WAITLIST } from "../config";

const tiers = [
  {
    name: "Free",
    price: "$0",
    featured: false,
    points: [
      "$10 credit included (no expiry)",
      "1 workspace",
      "Automated pull request generation",
      "Built-in harness validation in PR flow",
      "Open-source model path included",
      "Community support",
      "BYOK/BYOM support",
    ],
  },
  {
    name: "Scale",
    price: "Coming soon",
    featured: true,
    points: [
      "Everything in Free, plus:",
      "Multiple repos",
      "Multiple harness playbooks",
      "Save learnings across runs",
      "Bring your own frontier models",
      "Longer runtime and higher concurrency",
      "Usage-based billing for completed agent execution",
      "No charge for failed or incomplete agent runs",
      "Custom harness controls",
      "Custom agent profiles per workflow",
      "Team-level run observability and replay",
      "Sandboxed execution with tighter isolation",
      "Priority support",
    ],
  },
];

export default function Pricing() {
  return (
    <section className="stack">
      <p className="eyebrow">pricing</p>
      <h1>Free and Scale plans</h1>
      <p className="lead">
        UseZombie never resells model tokens. Bring your own keys/models and pay providers directly.
        UseZombie charges for agent compute runtime with clear plan boundaries.
      </p>

      <div className="grid two">
        {tiers.map((tier) => (
          <article key={tier.name} className={`card${tier.featured ? " featured" : ""}`}>
            <h2>{tier.name}</h2>
            <p className="price">{tier.price}</p>
            <ul className="pricing-points">
              {tier.points.map((point) => (
                <li key={point}>{point}</li>
              ))}
            </ul>
            {tier.name === "Scale" ? (
              <Button variant="ghost" to={MAILTO_SCALE_WAITLIST} style={{ marginTop: "0.75rem" }}>
                Join waitlist — I want this now
              </Button>
            ) : (
              <Button to={DOCS_QUICKSTART_URL} style={{ marginTop: "0.75rem" }}>
                Start free
              </Button>
            )}
          </article>
        ))}
      </div>

      <p className="fine">
        All plans include BYOK/BYOM and direct provider billing for token usage. Rate limits, abuse checks, and policy controls apply to all plans.
      </p>

      <FAQ />

      <div className="cta-block">
        <h2>Not sure which plan?</h2>
        <p>Start with Free for fast onboarding. Move to Scale for multi-repo orchestration, richer harness control, and saved team learnings.</p>
        <div className="cta-row">
          <a className="cta" href={DOCS_QUICKSTART_URL}>
            Start free
          </a>
          <a className="cta ghost" href={MAILTO_SCALE_WAITLIST}>
            Join Scale waitlist
          </a>
        </div>
      </div>
    </section>
  );
}
