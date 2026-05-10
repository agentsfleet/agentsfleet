import { Badge, Button, Card, List, ListItem, SectionLabel } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupStarted } from "../analytics/posthog";
import { RATES_DISPLAY } from "../lib/rates";

type FlowCell = { id: string; label: string; price: string; sub: string };

// Concrete platform-ops run — what an end user actually sees when a
// deploy webhook fires. Mirrors the install transcript on Hero.tsx;
// "stage N" lands on the resolving step (Slack diagnosis posted).
const BILLED_FLOW: FlowCell[] = [
  { id: "event", label: "event", price: RATES_DISPLAY.EVENT_RATE, sub: "deploy webhook fires" },
  { id: "stage-1", label: "stage 1", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "read CI logs" },
  { id: "stage-2", label: "stage 2", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "correlate commits" },
  { id: "stage-n", label: "stage N", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "post Slack diagnosis" },
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
              → try free · {RATES_DISPLAY.STARTER_CREDIT} starter credit, never expires
            </Badge>

            <p
              data-testid="pricing-rate-line"
              className="font-mono text-[clamp(28px,4vw,40px)] leading-[1.1] tracking-[-0.02em] font-medium text-text m-0 tabular-nums"
            >
              <span data-testid="pricing-rate-event">{RATES_DISPLAY.EVENT_RATE}</span>{" "}
              <span className="font-sans text-text-muted text-[18px] align-middle">per event receipt</span>
            </p>

            <div data-testid="pricing-stage-rates" className="flex flex-col gap-1.5">
              <p
                className="font-mono text-[clamp(24px,3.4vw,34px)] leading-[1.1] tracking-[-0.02em] font-medium text-text m-0 tabular-nums flex flex-wrap items-baseline gap-x-3 gap-y-1"
              >
                <span data-testid="pricing-rate-stage-platform">{RATES_DISPLAY.STAGE_PLATFORM}</span>
                <span className="font-sans text-text-muted text-[14px]">platform default</span>
                <span className="text-text-subtle">·</span>
                <span data-testid="pricing-rate-stage-self-managed">
                  {RATES_DISPLAY.STAGE_SELF_MANAGED}
                </span>
                <span className="font-sans text-text-muted text-[14px]">self-managed</span>
              </p>
              <p className="font-sans text-[14px] leading-[1.55] text-text-muted m-0">
                per stage execution — self-managed is 10× cheaper to scale once you bring your
                own provider key.
              </p>
              <p
                data-testid="pricing-introductory-rate-note"
                className="font-mono text-[11px] uppercase tracking-[0.08em] text-text-subtle m-0"
              >
                stealth-mode testing rate — will rise post-GA
              </p>
            </div>

            <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0 max-w-[640px]">
              A <span className="text-text">stage</span> is one reasoning step. Most diagnoses
              resolve in 1–5 stages.
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
              LLM call · your provider · your bill
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
