import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import OperationalKnowledgeSection from "../components/OperationalKnowledgeSection";
import PrebuiltAgents from "../components/PrebuiltAgents";
import HowItWorks from "../components/HowItWorks";
import Pricing from "../components/Pricing";
import CTABlock from "../components/CTABlock";
import FAQ from "../components/FAQ";
import { Card, DisplayLG, SectionLabel } from "@agentsfleet/design-system";
import {
  AGENT_PILLARS,
  CAPABILITY_HEADING,
  CAPABILITY_ITEMS,
} from "../lib/marketing-copy";

export default function Home() {
  return (
    <div data-testid="home-page">
      <Hero />
      <OperationalKnowledgeSection />
      <PrebuiltAgents />

      <section className="site-section" aria-label="Core capabilities">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">core capabilities</SectionLabel>
            <DisplayLG className="max-w-form">
              {CAPABILITY_HEADING}
            </DisplayLG>
          </div>

          {/* The three behavioral pillars — what every teammate is. Moved here
           * from the prebuilt-agents wall so capabilities lead the section, and
           * the prebuilts stay a clean catalogue. */}
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {AGENT_PILLARS.map((pillar) => (
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

          {/* The trust primitives — the guarantees underneath every run. */}
          <div className="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-4">
            {CAPABILITY_ITEMS.map((f) => (
              <FeatureSection
                key={f.number}
                number={f.number}
                title={f.title}
                description={f.description}
              />
            ))}
          </div>
        </div>
      </section>

      <HowItWorks />

      <Pricing />

      <FAQ />
      <CTABlock />
    </div>
  );
}
