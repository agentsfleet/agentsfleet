import { Badge, Button, Card, List, ListItem, SectionLabel } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackSignupStarted } from "../analytics/posthog";
import { SUPPORT_EMAIL } from "../lib/contact";
import { FREE_TRIAL_STAGE_DISPLAY, isWithinFreeTrial, RATES_DISPLAY } from "../lib/rates";

type FlowCell = {
  id: string;
  label: string;
  price: string;
  sub: string;
  kind: "event" | "stage";
};

// Concrete platform-ops run — what an end user actually sees when a
// deploy webhook fires. Mirrors the install transcript on Hero.tsx;
// "stage N" lands on the resolving step (Slack diagnosis posted).
// `price` is the post-trial rate; stage cells render "free" while the
// trial window is open (see isWithinFreeTrial), so the cards never
// contradict the "free until …" headline above them.
const BILLED_FLOW: FlowCell[] = [
  { id: "event", label: "event", price: RATES_DISPLAY.EVENT_RATE, sub: "deploy webhook fires", kind: "event" },
  { id: "stage-1", label: "stage 1", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "read CI logs", kind: "stage" },
  { id: "stage-2", label: "stage 2", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "correlate commits", kind: "stage" },
  { id: "stage-n", label: "stage N", price: RATES_DISPLAY.STAGE_PLATFORM, sub: "post Slack diagnosis", kind: "stage" },
];

const EXTRAS: string[] = [
  "multi-workspace with shared event history",
  "approval gating in dashboard and Slack DM",
  "workspace-scoped credentials and webhooks",
  "higher concurrency and longer per-stage windows — lift caps on request",
  "priority support",
];

export default function Pricing() {
  // While the trial window is open, stage cells render "free" to match the
  // headline; after it closes they show the per-stage rate.
  const trial = isWithinFreeTrial();
  return (
    <section id="pricing" className="site-section" data-testid="pricing-block">
      <div className="wrap flex flex-col gap-10">
        <Card data-testid="pricing-rate-card" className="flex flex-col gap-5">
          <div className="flex flex-col gap-3">
            <Badge className="self-start font-mono">
              → try free · free until July 2026
            </Badge>

            <p
              data-testid="pricing-free-trial-banner"
              className="font-sans text-body-sm leading-body text-text-muted m-0 max-w-narrow"
            >
              {RATES_DISPLAY.FREE_TRIAL_BANNER}
            </p>

            <p
              data-testid="pricing-rate-line"
              className="font-mono text-fluid-display-lg leading-display-lg tracking-display-lg font-medium text-text m-0 tabular-nums"
            >
              <span data-testid="pricing-rate-event">{RATES_DISPLAY.EVENT_RATE}</span>{" "}
              <span className="font-sans text-text-muted text-body-lg align-middle">per event receipt</span>
            </p>

            <div data-testid="pricing-stage-rates" className="flex flex-col gap-1.5">
              <p
                className="font-mono text-fluid-display-md leading-display-md tracking-display-md font-medium text-text m-0 tabular-nums flex flex-wrap items-baseline gap-x-3 gap-y-1"
              >
                <s data-testid="pricing-rate-stage-platform" className="text-text-muted">
                  {RATES_DISPLAY.STAGE_PLATFORM}
                </s>
                <span className="font-sans text-text-muted text-body-sm">platform default</span>
                <span className="text-text-subtle">·</span>
                <s data-testid="pricing-rate-stage-self-managed" className="text-text-muted">
                  {RATES_DISPLAY.STAGE_SELF_MANAGED}
                </s>
                <span className="font-sans text-text-muted text-body-sm">self-managed</span>
              </p>
              <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                per stage execution — self-managed is 10× cheaper to scale once you bring your
                own provider key.
              </p>
              <p
                data-testid="pricing-introductory-rate-note"
                className="font-mono text-label uppercase tracking-label text-text-subtle m-0"
              >
                stealth-mode testing rate — will rise post-GA
              </p>
            </div>

            <p className="font-sans text-body leading-body text-text-muted m-0 max-w-narrow">
              A <span className="text-text">stage</span> is one reasoning step. Most diagnoses
              resolve in 1–5 stages.
            </p>

            <p
              data-testid="pricing-design-partner-note"
              className="font-sans text-body-sm leading-body-sm text-pulse m-0 max-w-narrow"
            >
              Stealth-mode testing — APIs and behavior change without long deprecation windows.
              Want a hand calibrating a zombie or to join as a design partner? Email{" "}
              <a
                href={`mailto:${SUPPORT_EMAIL}?subject=Design%20partner`}
                className="underline hover:text-text"
              >
                {SUPPORT_EMAIL}
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
              → get early access
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
                <span className="font-mono text-heading leading-heading tabular-nums text-text">
                  {trial && cell.kind === "stage" ? FREE_TRIAL_STAGE_DISPLAY : cell.price}
                </span>
                <span className="font-sans text-eyebrow leading-eyebrow text-text-muted">
                  {cell.sub}
                </span>
              </div>
            ))}
          </div>

          {trial && (
            <p
              className="font-sans text-body-sm leading-body text-text-muted m-0"
              data-testid="pricing-flow-trial-note"
            >
              Stages are free during the trial — billed at {RATES_DISPLAY.STAGE_PLATFORM}/stage
              once the free window closes.
            </p>
          )}

          <div
            data-testid="pricing-flow-llm"
            className="flex flex-col gap-2 p-4 border border-dashed border-border"
          >
            <SectionLabel className="mb-0">
              underneath every stage — not on your usezombie bill
            </SectionLabel>
            <span className="font-mono text-body-sm text-text">
              LLM call · your provider · your bill
            </span>
            <span className="font-sans text-eyebrow leading-body-lg text-text-muted">
              Anthropic · OpenAI · Fireworks · Together · Groq · Moonshot. Pay your provider
              directly; usezombie marks up zero on inference.
            </span>
          </div>

          <p className="font-sans text-body-sm leading-body text-text-muted m-0">
            One event wakes the zombie. The runtime executes one or more stages until the outcome
            is resolved or blocked. Each stage is independently billed; the model call rides
            underneath and never touches your usezombie invoice.
          </p>
        </Card>

        <div className="flex flex-col gap-3 max-w-form">
          <SectionLabel className="mb-0">
            operational extras — provisioned per workspace as you scale, not gated by tier
          </SectionLabel>
          <List variant="plain" data-testid="pricing-extras" className="flex flex-col gap-2">
            {EXTRAS.map((point) => (
              <ListItem
                key={point}
                bullet="dot"
                className="font-mono text-mono leading-mono text-text-muted"
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
