import { DisplayLG, List, ListItem, Section, SectionLabel } from "@agentsfleet/design-system";

const AUDIENCES = [
  {
    id: "founders",
    eyebrow: "For solo founders & small teams",
    title: "One less job pulling you away from building.",
    detail: "Start with a second pass on your pull requests or the investigation you keep putting off.",
    points: ["Get review comments where you already work.", "Gather incident evidence without opening every dashboard yourself.", "Keep the final call on fixes and releases."],
  },
  {
    id: "infra",
    eyebrow: "For infrastructure leads",
    title: "Delegate the investigation. Keep the controls.",
    detail: "Put a bounded fleet alongside your existing tools, with explicit access and a result your team can inspect.",
    points: ["Choose repositories, data sources, and permissions.", "Approve repository writes before repair begins.", "Inspect run history, evidence, and fleet spending."],
  },
] as const;

export function AudienceSection() {
  return (
    <Section asChild className="site-section" data-testid="audience-section">
      <section aria-label="Built for founders and infrastructure teams">
        <div className="wrap audience-layout">
          {AUDIENCES.map((audience) => (
            <div key={audience.id} className="audience-panel" data-testid={`audience-${audience.id}`}>
              <SectionLabel className="mb-0">{audience.eyebrow}</SectionLabel>
              <DisplayLG>{audience.title}</DisplayLG>
              <p className="m-0 text-body-lg text-text-muted">{audience.detail}</p>
              <List variant="plain" className="m-0 space-y-3">
                {audience.points.map((point) => <ListItem key={point} bullet="arrow" className="text-body-sm text-text-muted">{point}</ListItem>)}
              </List>
            </div>
          ))}
        </div>
      </section>
    </Section>
  );
}
