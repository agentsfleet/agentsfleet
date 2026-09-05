import { DisplayLG, Section, SectionLabel } from "@agentsfleet/design-system";
import { FleetPreview } from "./FleetPreview";
import { HOW_IT_WORKS_FOOTNOTE, HOW_IT_WORKS_HEADING } from "../lib/marketing-copy";

export default function HowItWorks() {
  return (
    <Section asChild className="site-section" data-testid="how-it-works">
      <section aria-label="How it works">
        <div className="wrap flex flex-col gap-8">
          <div className="section-intro">
            <div className="flex flex-col gap-3">
              <SectionLabel className="mb-0">How it works</SectionLabel>
              <DisplayLG className="max-w-narrow">{HOW_IT_WORKS_HEADING}</DisplayLG>
            </div>
            <p className="m-0 text-body-lg text-text-muted max-w-form">Your dashboards hold the clues. Your fleet brings them together so your team can decide what to do next.</p>
          </div>
          <FleetPreview />
          <p className="m-0 text-body-sm text-text-muted max-w-measure">{HOW_IT_WORKS_FOOTNOTE}</p>
        </div>
      </section>
    </Section>
  );
}
