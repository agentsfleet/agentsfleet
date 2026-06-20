import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import OperationalKnowledgeSection from "../components/OperationalKnowledgeSection";
import PrebuiltFleets from "../components/PrebuiltFleets";
import HowItWorks from "../components/HowItWorks";
import Pricing from "../components/Pricing";
import CTABlock from "../components/CTABlock";
import FAQ from "../components/FAQ";
import {
  Card,
  DisplayLG,
  List,
  ListItem,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import {
  FLEET_PILLARS,
  CAPABILITY_HEADING,
  CAPABILITY_ITEMS,
  RUNTIME_GUARANTEES_LABEL,
} from "../lib/marketing-copy";

export default function Home() {
  return (
    <div data-testid="home-page">
      <Hero />
      <CoreCapabilitiesSection />
      <PrebuiltFleets />
      <HowItWorks />
      <OperationalKnowledgeSection />

      <Pricing />

      <FAQ />
      <CTABlock />
    </div>
  );
}

function CoreCapabilitiesSection() {
  return (
    <Section asChild className="site-section" data-testid="core-capabilities">
      <section aria-label="Core capabilities">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">core capabilities</SectionLabel>
            <DisplayLG className="max-w-form">
              {CAPABILITY_HEADING}
            </DisplayLG>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {FLEET_PILLARS.map((pillar) => (
              <Card
                key={pillar.id}
                className="flex flex-col gap-2"
                data-testid={`capability-pillar-${pillar.id}`}
              >
                <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-pulse">
                  {pillar.eyebrow}
                </span>
                <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
                  {pillar.title}
                </h3>
                <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                  {pillar.description}
                </p>
              </Card>
            ))}
          </div>

          <div className="flex flex-col gap-3" aria-label="Runtime guarantees">
            <h3 className="font-mono text-label uppercase tracking-label text-text-subtle m-0">
              {RUNTIME_GUARANTEES_LABEL}
            </h3>
            <Card className="lg:hidden">
              <List variant="plain" divided className="m-0 space-y-0">
                {CAPABILITY_ITEMS.map((item) => (
                  <ListItem key={item.number} className="py-3 first:pt-0 last:pb-0">
                    <div className="flex flex-col gap-1">
                      <span className="font-mono text-label uppercase tracking-label text-text-subtle">
                        {item.number}
                      </span>
                      <span className="font-mono text-heading leading-heading text-text font-medium">
                        {item.title}
                      </span>
                      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                        {item.description}
                      </p>
                    </div>
                  </ListItem>
                ))}
              </List>
            </Card>
            <div className="hidden grid-cols-4 gap-3 lg:grid">
              {CAPABILITY_ITEMS.map((item) => (
                <FeatureSection
                  key={item.number}
                  number={item.number}
                  title={item.title}
                  description={item.description}
                  compact
                />
              ))}
            </div>
          </div>
        </div>
      </section>
    </Section>
  );
}
