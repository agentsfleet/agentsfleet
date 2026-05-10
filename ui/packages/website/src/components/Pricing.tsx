import { Badge, Button, Card, List, ListItem, SectionLabel } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupStarted } from "../analytics/posthog";
import { RATES_DISPLAY, WORKED_EXAMPLE } from "../lib/rates";

type FlowCell = { id: string; label: string; price: string; sub: string };

const BILLED_FLOW: FlowCell[] = [
  { id: "event", label: "event", price: RATES_DISPLAY.eventPlatform, sub: "webhook · cron · steer" },
  { id: "stage-1", label: "stage 1", price: RATES_DISPLAY.stage, sub: "reason · act" },
  { id: "stage-2", label: "stage 2", price: RATES_DISPLAY.stage, sub: "reason · act" },
  { id: "stage-n", label: "stage N", price: RATES_DISPLAY.stage, sub: "until resolved" },
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
            <Badge className="self-start font-mono">
              → try free · {RATES_DISPLAY.starterCredit} starter credit, never expires
            </Badge>
            <p
              data-testid="pricing-rate-line"
              className="font-mono text-[clamp(28px,4vw,40px)] leading-[1.1] tracking-[-0.02em] font-medium text-text m-0 tabular-nums"
            >
              <span data-testid="pricing-rate-event">{RATES_DISPLAY.eventPlatform}</span>{" "}
              <span className="font-sans text-text-muted text-[18px] align-middle">per event receipt</span>
              <span className="text-text-subtle"> · </span>
              <span data-testid="pricing-rate-stage">{RATES_DISPLAY.stage}</span>{" "}
              <span className="font-sans text-text-muted text-[18px] align-middle">per stage execution</span>
            </p>
            <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0 max-w-[640px]">
              A <span className="text-text">stage</span> is one reasoning step — the agent gathers
              evidence, decides, or acts once, then either stops or continues into the next stage.
              Most diagnoses resolve in 1–5 stages. Self-managed on Anthropic, OpenAI, Fireworks,
              Together, Groq, Moonshot — your provider bills you for tokens directly; usezombie
              never marks up inference.
            </p>
            <p
              data-testid="pricing-design-partner-note"
              className="font-sans text-[13px] leading-[1.55] text-pulse m-0 max-w-[640px]"
            >
              Stealth-mode testing — APIs and behavior change without long deprecation windows.
              Want a hand calibrating a zombie or to join as a design partner? Email{" "}
              <a
                href="mailto:usezombie@agentmail.to?subject=Design%20partner"
                className="underline hover:text-text"
              >
                usezombie@agentmail.to
              </a>
              .
            </p>
          </div>

          <p
            data-testid="pricing-worked-example"
            className="font-mono text-[14px] leading-[1.6] text-text-muted m-0 border-l-2 border-border pl-4"
          >
            {WORKED_EXAMPLE.events} events with {WORKED_EXAMPLE.stagesPerEvent} stages each ={" "}
            {WORKED_EXAMPLE.events} × {RATES_DISPLAY.eventPlatform} +{" "}
            {WORKED_EXAMPLE.events * WORKED_EXAMPLE.stagesPerEvent} × {RATES_DISPLAY.stage} ={" "}
            <span className="text-text">{WORKED_EXAMPLE.total}</span>. Your{" "}
            {RATES_DISPLAY.starterCredit} starter credit covers ~{WORKED_EXAMPLE.starterCoversEvents}{" "}
            events at this shape.
          </p>

          <Button asChild>
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
              LLM call · Self-managed · your bill
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
