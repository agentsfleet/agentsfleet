import { Badge, Button, Card, List, ListItem, SectionLabel } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupCompleted } from "../analytics/posthog";

type FlowCell = { id: string; label: string; price: string; sub: string };

const BILLED_FLOW: FlowCell[] = [
  { id: "event", label: "event", price: "$0.001", sub: "webhook · cron · steer" },
  { id: "stage-1", label: "stage 1", price: "$0.10", sub: "reason · act" },
  { id: "stage-2", label: "stage 2", price: "$0.10", sub: "reason · act" },
  { id: "stage-n", label: "stage N", price: "$0.10", sub: "until resolved" },
];

const EXTRAS: string[] = [
  "multi-workspace with shared event history",
  "approval gating in dashboard and Slack DM",
  "workspace-scoped credentials and webhooks",
  "higher concurrency and longer per-stage windows — lift caps on request",
  "priority support",
];

export default function Pricing() {
  return (
    <section id="pricing" className="site-section" data-testid="pricing-block">
      <div className="wrap flex flex-col gap-10">
        <Card data-testid="pricing-rate-card" className="flex flex-col gap-5">
          <div className="flex flex-col gap-3">
            <Badge className="self-start font-mono">$5 starter credit, never expires</Badge>
            <p
              data-testid="pricing-rate-line"
              className="font-mono text-[clamp(28px,4vw,40px)] leading-[1.1] tracking-[-0.02em] font-medium text-text m-0 tabular-nums"
            >
              <span data-testid="pricing-rate-event">$0.001</span>{" "}
              <span className="font-sans text-text-muted text-[18px] align-middle">per event receipt</span>
              <span className="text-text-subtle"> · </span>
              <span data-testid="pricing-rate-stage">$0.10</span>{" "}
              <span className="font-sans text-text-muted text-[18px] align-middle">per stage execution</span>
            </p>
            <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0 max-w-[640px]">
              BYOK on Anthropic, OpenAI, Fireworks, Together, Groq, Moonshot. Pay your provider
              directly — usezombie never marks up tokens.
            </p>
          </div>

          <p
            data-testid="pricing-worked-example"
            className="font-mono text-[14px] leading-[1.6] text-text-muted m-0 border-l-2 border-border pl-4"
          >
            100 events with 3 stages each = 100 × $0.001 + 300 × $0.10 ={" "}
            <span className="text-text">$30.10</span>. Your $5 starter credit covers ~16 events at
            this shape.
          </p>

          <Button asChild>
            <a
              href={APP_BASE_URL}
              data-testid="pricing-install-cta"
              onClick={() =>
                trackSignupCompleted({
                  source: "pricing_install",
                  surface: "pricing",
                  mode: "humans",
                })
              }
            >
              → install
            </a>
          </Button>
        </Card>

        <Card data-testid="pricing-flow" className="flex flex-col gap-5">
          <SectionLabel className="mb-0">how a run is billed</SectionLabel>

          <div
            data-testid="pricing-flow-billed"
            aria-label="Per-run billing flow: one event plus N stages"
            className="grid gap-3 grid-cols-1 sm:grid-cols-2 lg:grid-cols-4"
          >
            {BILLED_FLOW.map((cell) => (
              <div
                key={cell.id}
                data-testid={`pricing-flow-cell-${cell.id}`}
                className="flex flex-col gap-1 p-4 border border-border bg-surface-1"
              >
                <SectionLabel className="mb-0">{cell.label}</SectionLabel>
                <span className="font-mono text-[20px] leading-[1.1] tabular-nums text-text">
                  {cell.price}
                </span>
                <span className="font-sans text-[12px] leading-[1.45] text-text-muted">
                  {cell.sub}
                </span>
              </div>
            ))}
          </div>

          <div
            data-testid="pricing-flow-llm"
            className="flex flex-col gap-2 p-4 border border-dashed border-border"
          >
            <SectionLabel className="mb-0">
              underneath every stage — not on your usezombie bill
            </SectionLabel>
            <span className="font-mono text-[14px] text-text">
              LLM call · BYOK · your bill
            </span>
            <span className="font-sans text-[12px] leading-[1.5] text-text-muted">
              Anthropic · OpenAI · Fireworks · Together · Groq · Moonshot. Pay your provider
              directly; usezombie marks up zero on inference.
            </span>
          </div>

          <p className="font-sans text-[13px] leading-[1.55] text-text-muted m-0">
            One event wakes the zombie. The runtime executes one or more stages until the outcome
            is resolved or blocked. Each stage is independently billed; the model call rides
            underneath and never touches your usezombie invoice.
          </p>
        </Card>

        <div className="flex flex-col gap-3 max-w-[760px]">
          <SectionLabel className="mb-0">
            operational extras — provisioned per workspace as you scale, not gated by tier
          </SectionLabel>
          <List variant="plain" data-testid="pricing-extras" className="flex flex-col gap-2">
            {EXTRAS.map((point) => (
              <ListItem
                key={point}
                className="font-mono text-[13px] leading-[1.5] text-text-muted before:content-['·_'] before:text-text-subtle"
              >
                {point}
              </ListItem>
            ))}
          </List>
        </div>
      </div>
    </section>
  );
}
