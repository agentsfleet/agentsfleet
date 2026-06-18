import { Card, DisplayLG, Section, SectionLabel } from "@agentsfleet/design-system";
import {
  KNOWLEDGE_POINTS,
  OPERATIONAL_KNOWLEDGE_HEADING,
  OPERATIONAL_KNOWLEDGE_LEDE,
  type KnowledgePoint,
} from "../lib/marketing-copy";

export default function OperationalKnowledgeSection() {
  return (
    <Section asChild className="site-section" data-testid="operational-knowledge">
      <section aria-label="Operational knowledge">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">operational knowledge</SectionLabel>
            <DisplayLG className="max-w-narrow">
              {OPERATIONAL_KNOWLEDGE_HEADING}
            </DisplayLG>
            <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-measure">
              {OPERATIONAL_KNOWLEDGE_LEDE}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {KNOWLEDGE_POINTS.map((point) => (
              <KnowledgeCard key={point.number} point={point} />
            ))}
          </div>
        </div>
      </section>
    </Section>
  );
}

function KnowledgeCard({ point }: { point: KnowledgePoint }) {
  return (
    <Card className="flex flex-col gap-3" data-testid={`knowledge-point-${point.number}`}>
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        {point.number}
      </span>
      <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
        {point.title}
      </h3>
      <p className="font-sans text-body leading-body text-text-muted m-0">
        {point.description}
      </p>
    </Card>
  );
}
