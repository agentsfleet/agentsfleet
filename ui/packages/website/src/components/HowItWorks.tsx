import { Card, DisplayLG, SectionLabel } from "@agentsfleet/design-system";
import { HOW_IT_WORKS_HEADING, LOOP_STEPS } from "../lib/marketing-copy";

/*
 * HowItWorks — LOOP_STEPS-driven mono numbered cards. No counter
 * pseudo-element, no orange glow on hover. Border-only elevation.
 */
export default function HowItWorks() {
  return (
    <section className="site-section" aria-label="How it works" data-testid="how-it-works">
      <div className="wrap flex flex-col gap-8">
        <div className="flex flex-col gap-3">
          <SectionLabel className="mb-0">How it works</SectionLabel>
          <DisplayLG className="max-w-narrow">
            {HOW_IT_WORKS_HEADING}
          </DisplayLG>
        </div>
        <div className="grid gap-4 grid-cols-[repeat(auto-fit,minmax(260px,1fr))]">
          {LOOP_STEPS.map((step) => (
            <Card
              key={step.number}
              className="flex flex-col gap-3"
              data-testid={`how-step-${step.number}`}
            >
              <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
                {step.number}
              </span>
              <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
                {step.title}
              </h3>
              <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                {step.description}
              </p>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
