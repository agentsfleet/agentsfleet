import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import OperationalKnowledgeSection from "../components/OperationalKnowledgeSection";
import PrebuiltAgents from "../components/PrebuiltAgents";
import HowItWorks from "../components/HowItWorks";
import Pricing from "../components/Pricing";
import CTABlock from "../components/CTABlock";
import FAQ from "../components/FAQ";
import { DisplayLG, SectionLabel } from "@agentsfleet/design-system";
import { CAPABILITY_HEADING, CAPABILITY_ITEMS } from "../lib/marketing-copy";

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
