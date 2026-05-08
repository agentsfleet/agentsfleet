import FAQ from "../components/FAQ";
import { Badge, Button, Card, List, ListItem } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupCompleted, trackLeadCaptureClicked } from "../analytics/posthog";

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
  "open source runtime",
  "three triggers, one reasoning loop",
  "self-host arrives in v3",
];

const tiers: Tier[] = [
  {
    name: "Hobby",
    availability: "available now",
    price: "Free",
    audience: "For solo operators evaluating the wedge.",
    ctaSource: "pricing_hobby_start_free",
    ctaLabel: "→ start free",
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
    availability: "upgrade when ready",
    price: "From Mission Control",
    audience: "For teams running agents across shared workspaces.",
    featured: true,
    ctaSource: "pricing_scale_upgrade",
    ctaLabel: "→ upgrade in app",
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
    <div data-testid="pricing-page">
      <section className="site-section">
        <div className="wrap flex flex-col gap-8">
          <p className="font-mono text-[12px] uppercase tracking-[0.1em] text-text-muted m-0">
            pricing
          </p>
          <h1 className="font-mono text-[clamp(40px,6vw,64px)] leading-[1.05] tracking-[-0.025em] font-medium text-text m-0 max-w-[900px]">
            Start free. Upgrade when you need stronger control.
          </h1>
          <p className="font-sans text-[18px] leading-[1.5] text-text-muted m-0 max-w-[640px]">
            usezombie sells durable execution and operational ownership — not marked-up model
            usage. Bring your own model key; pay your provider directly. Hosted execution is
            metered against a credit pool with a $5 starter grant — debits fire on event receipt
            and per-stage execution.
          </p>

          <div className="flex flex-wrap gap-2" aria-label="Pricing proof points">
            {roadmapSignals.map((signal) => (
              <Badge key={signal} className="font-mono">
                {signal}
              </Badge>
            ))}
          </div>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap grid gap-6 grid-cols-1 lg:grid-cols-2">
          {tiers.map((tier) => (
            <Card
              key={tier.name}
              data-testid={`pricing-card-${tier.name.toLowerCase()}`}
              data-featured={tier.featured ? "true" : undefined}
              className={`flex flex-col gap-6 ${tier.featured ? "border-pulse" : ""}`}
            >
              <div className="flex flex-col gap-3">
                <Badge className="self-start font-mono">
                  {tier.availability}
                </Badge>
                <h2 className="font-mono text-[24px] leading-[1.2] tracking-[-0.02em] font-medium text-text m-0">
                  {tier.name}
                </h2>
                <p className="font-sans text-[14px] leading-[1.5] text-text-muted m-0">
                  {tier.audience}
                </p>
                <p
                  className="font-mono text-[clamp(36px,5vw,52px)] leading-[1] tracking-[-0.025em] font-medium text-text m-0 tabular-nums"
                  data-testid={`pricing-price-${tier.name.toLowerCase()}`}
                >
                  {tier.price}
                </p>
                {tier.priceNote ? (
                  <p className="font-sans text-[13px] text-text-subtle m-0">{tier.priceNote}</p>
                ) : null}
              </div>

              <Button asChild variant={tier.featured ? "default" : "ghost"}>
                <a
                  href={APP_BASE_URL}
                  onClick={() =>
                    tier.featured
                      ? trackLeadCaptureClicked({
                          page: "pricing",
                          surface: "pricing_card",
                          cta_id: tier.ctaSource,
                          plan_interest: "Scale",
                        })
                      : trackSignupCompleted({
                          source: tier.ctaSource,
                          surface: "pricing",
                          mode: "humans",
                        })
                  }
                >
                  {tier.ctaLabel}
                </a>
              </Button>

              <div className="flex flex-col gap-2 border-t border-border pt-4">
                <p className="font-mono text-[11px] uppercase tracking-[0.08em] text-text-subtle m-0">
                  why this plan
                </p>
                <p className="font-sans text-[14px] leading-[1.55] text-text-muted m-0">
                  {tier.proof}
                </p>
              </div>

              <List variant="plain" className="flex flex-col gap-2">
                {tier.highlights.map((point) => (
                  <ListItem
                    key={point}
                    className="font-mono text-[13px] leading-[1.5] text-text-muted before:content-['✓'] before:mr-2 before:text-success"
                  >
                    {point}
                  </ListItem>
                ))}
              </List>
            </Card>
          ))}
        </div>
      </section>

      <section className="site-section">
        <div className="wrap grid gap-6 grid-cols-1 lg:grid-cols-2 items-start max-w-[960px]">
          <div className="flex flex-col gap-3">
            <p className="font-mono text-[11px] uppercase tracking-[0.1em] text-text-muted m-0">
              when to move up
            </p>
            <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
              <span className="whitespace-nowrap">Start on Hobby.</span>{" "}
              <span className="whitespace-nowrap">Move to Scale</span> when agents
              become shared infrastructure.
            </h2>
          </div>
          <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0">
            Hobby validates the wedge on one repo. Scale adds shared event history, approval flows,
            and the budget controls a team needs once agents own real production outcomes.
          </p>
        </div>
      </section>

      <FAQ />
    </div>
  );
}
