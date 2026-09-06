import { DisplayLG, Section, SectionLabel, Accordion, AccordionItem, AccordionTrigger, AccordionContent } from "@agentsfleet/design-system";
import { FleetPreview } from "./FleetPreview";
import { HOW_IT_WORKS_ANCHOR_ID, HOW_IT_WORKS_FOOTNOTE, HOW_IT_WORKS_HEADING } from "../lib/marketing-copy";

const WORKFLOW = { incident: "incident", slack: "slack" } as const;

export default function HowItWorks() {
  return (
    <Section asChild className="site-section" data-testid="how-it-works">
      <section id={HOW_IT_WORKS_ANCHOR_ID} aria-label="How it works">
        <div className="wrap flex flex-col gap-8">
          <div className="section-intro">
            <div className="flex flex-col gap-3">
              <SectionLabel className="mb-0">How it works</SectionLabel>
              <DisplayLG className="max-w-narrow">{HOW_IT_WORKS_HEADING}</DisplayLG>
            </div>
            <p className="m-0 text-body-lg text-text-muted max-w-form">Your dashboards hold the clues. Your fleet brings them together so your team can decide what to do next.</p>
          </div>
          <Accordion type="single" collapsible defaultValue={WORKFLOW.incident}>
            <AccordionItem value={WORKFLOW.incident}>
              <AccordionTrigger>Incident Response</AccordionTrigger>
              <AccordionContent className="flex flex-col gap-4">
              <FleetPreview />
              <p className="m-0 text-body-sm text-text-muted max-w-measure">{HOW_IT_WORKS_FOOTNOTE}</p>
              </AccordionContent>
            </AccordionItem>
            <AccordionItem value={WORKFLOW.slack}>
              <AccordionTrigger>Slack Teammate</AccordionTrigger>
              <AccordionContent><SlackExample /></AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </section>
    </Section>
  );
}

function SlackExample() {
  return (
    <figure aria-label="Illustrated example: Slack teammate" className="m-0 rounded-lg border border-border bg-card p-6">
      <figcaption className="mb-6 text-body-sm text-text-muted">Illustrative example · Slack Teammate</figcaption>
      <p className="font-medium text-text">You mention @agentsfleet in your channel.</p>
      <blockquote className="my-4 border-l-2 border-pulse pl-4 text-text-muted">What did we learn from the last checkout incident?</blockquote>
      <p className="text-text">Your teammate answers in the thread using that channel’s saved context.</p>
      <p className="mt-4 text-body-sm text-text-muted">Invite it to the channel and mention it when you need help. It stays read-only and does not read other channels’ memory.</p>
    </figure>
  );
}
