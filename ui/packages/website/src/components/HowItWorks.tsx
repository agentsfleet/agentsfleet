import { Card, DisplayLG, SectionLabel } from "@usezombie/design-system";

const steps = [
  {
    n: "01",
    title: "A trigger arrives",
    description:
      "A GitHub Actions deploy fails, a cron fires, or you run zombiectl steer. Each lands on the event stream with actor provenance: webhook:github, cron:<schedule>, steer:<user>.",
  },
  {
    n: "02",
    title: "The agent gathers evidence",
    description:
      "It calls the tools TRIGGER.md allow-lists — http_request, memory_store, cron_add. Secrets substitute at the sandbox boundary; the model sees placeholders, never raw bytes.",
  },
  {
    n: "03",
    title: "Diagnosis posts; the run is auditable",
    description:
      "Slack receives the evidenced diagnosis. Every event is on core.zombie_events with actor and timestamp. zombiectl steer {id} picks the conversation up later.",
  },
];

/*
 * HowItWorks — 3-step mono numbered cards. No counter pseudo-element,
 * no orange glow on hover. Border-only elevation.
 */
export default function HowItWorks() {
  return (
    <section className="site-section" aria-label="How it works" data-testid="how-it-works">
      <div className="wrap flex flex-col gap-8">
        <div className="flex flex-col gap-3">
          <SectionLabel className="mb-0">How it works</SectionLabel>
          <DisplayLG className="max-w-[640px]">
            From trigger to evidenced diagnosis, durably.
          </DisplayLG>
        </div>
        <div className="grid gap-4 grid-cols-[repeat(auto-fit,minmax(260px,1fr))]">
          {steps.map((step) => (
            <Card key={step.n} className="flex flex-col gap-3" data-testid={`how-step-${step.n}`}>
              <span className="font-mono text-[12px] uppercase tracking-[0.08em] text-text-subtle">
                {step.n}
              </span>
              <h3 className="font-mono text-[16px] leading-[1.3] tracking-[-0.01em] text-text font-medium m-0">
                {step.title}
              </h3>
              <p className="font-sans text-[14px] leading-[1.55] text-text-muted m-0">
                {step.description}
              </p>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
