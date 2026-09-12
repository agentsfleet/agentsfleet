import Hero from "../components/Hero";
import PrebuiltFleets from "../components/PrebuiltFleets";
import HowItWorks from "../components/HowItWorks";
import Pricing from "../components/Pricing";
import CTABlock from "../components/CTABlock";
import FAQ from "../components/FAQ";
import { AudienceSection } from "../components/AdoptionSections";
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
      <HowItWorks />
      <PrebuiltFleets />
      <AudienceSection />
      <CoreCapabilitiesSection />
      <Pricing />

      <FAQ />
      <CTABlock />
    </div>
  );
}

// The id the runtime-guarantees group is named by.
const RUNTIME_GUARANTEES_HEADING_ID = "runtime-guarantees-heading";

function CoreCapabilitiesSection() {
  return (
    <Section asChild className="site-section" data-testid="core-capabilities">
      <section aria-label="Core capabilities">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel as="p" className="mb-0">core capabilities</SectionLabel>
            <DisplayLG className="max-w-form">
              {CAPABILITY_HEADING}
            </DisplayLG>
          </div>

          <CapabilityPillars />
          <RuntimeGuarantees />
        </div>
      </section>
    </Section>
  );
}

function CapabilityPillars() {
  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
      {FLEET_PILLARS.map((pillar) => (
        <Card
          key={pillar.id}
          className="flex flex-col gap-2"
          data-testid={`capability-pillar-${pillar.id}`}
        >
          <span className="font-sans text-eyebrow uppercase tracking-eyebrow text-pulse">
            {pillar.eyebrow}
          </span>
          <h3 className="font-sans text-heading leading-heading text-text font-medium m-0">
            {pillar.title}
          </h3>
          <p className="font-sans text-body-sm leading-body text-text-muted m-0">
            {pillar.description}
          </p>
        </Card>
      ))}
    </div>
  );
}

/*
 * One divided list, at every width.
 *
 * These four rendered as a four-across card grid from `lg`, sitting directly
 * under the three pillar cards — two symmetric card grids stacked in one
 * section, seven boxes of the same shape, none of them an interaction. Cards
 * earn their existence by being the thing you act on; these are prose, and
 * boxing prose is what makes a page look assembled from a template.
 *
 * The narrow-width treatment was already right: one card, hairline-divided
 * rows. It is now the only treatment, which also means `CAPABILITY_ITEMS` is
 * rendered once rather than twice under opposing breakpoint classes.
 *
 * The heading stays an <h3> — it heads a subsection of Core capabilities, and
 * that is its real rank. It names the group through `aria-labelledby`, because
 * the `aria-label` this carried sat on a bare <div>, which exposes no role for
 * a name to attach to and is dropped.
 */
function RuntimeGuarantees() {
  return (
    <section className="flex flex-col gap-3" aria-labelledby={RUNTIME_GUARANTEES_HEADING_ID}>
      {/*
       * FINDING-M03. This was an <h3> at 12px sitting among sibling <h3>s at
       * 20px — the pillar titles beside it. It is a label for a group, not a
       * peer of those titles, and rank should follow that. It is a <p> now;
       * the group keeps its accessible name because `aria-labelledby` above
       * points at this element, which works whatever tag it carries.
       */}
      <p
        id={RUNTIME_GUARANTEES_HEADING_ID}
        className="font-mono text-label uppercase tracking-label text-text-subtle m-0"
      >
        {RUNTIME_GUARANTEES_LABEL}
      </p>
      <Card>
        <List variant="plain" divided className="m-0 space-y-0">
          {CAPABILITY_ITEMS.map((item) => (
            <ListItem key={item.number} className="py-3 first:pt-0 last:pb-0">
              <div className="flex flex-col gap-1">
                <span className="font-mono text-label uppercase tracking-label text-text-subtle">
                  {item.number}
                </span>
                <span className="font-sans text-heading leading-heading text-text font-medium">
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
    </section>
  );
}

