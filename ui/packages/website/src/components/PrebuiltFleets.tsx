import {
  Button,
  Card,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
  DisplayLG,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { trackSignupStarted } from "../analytics/posthog";
import { WAITLIST_URL } from "../config";
import {
  FLEETS_SECTION_HEADING,
  FLEETS_SECTION_LEDE,
  LOOP_ANCHOR_ID,
  PREBUILT_FLEETS,
  type PrebuiltFleet,
} from "../lib/marketing-copy";

// Keep the shared anchor stable for hero, footer, and machine-readable links.
export default function PrebuiltFleets() {
  return (
    <Section asChild className="site-section" data-testid="prebuilt-fleets">
      <section id={LOOP_ANCHOR_ID} aria-label="Meet the fleet">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">The fleet</SectionLabel>
            <DisplayLG>{FLEETS_SECTION_HEADING}</DisplayLG>
            <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">
              {FLEETS_SECTION_LEDE}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            {PREBUILT_FLEETS.map((fleet) => (
              <FleetCard key={fleet.id} fleet={fleet} />
            ))}
          </div>
        </div>
      </section>
    </Section>
  );
}

function FleetCard({ fleet }: { fleet: PrebuiltFleet }) {
  return (
    <Card
      className="flex h-full flex-col gap-4"
      data-testid={`fleet-card-${fleet.id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-sans text-eyebrow uppercase tracking-eyebrow text-text-subtle">
          {fleet.category}
        </span>
        {fleet.comingSoon ? (
          <span
            className="rounded-sm border border-border px-2 py-0.5 font-mono text-label uppercase tracking-label text-text-subtle"
            data-testid={`fleet-coming-soon-${fleet.id}`}
          >
            Coming soon
          </span>
        ) : null}
      </div>
      <h3 className="font-sans text-heading leading-heading text-text font-medium m-0">
        {fleet.name}
      </h3>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        {fleet.description}
      </p>
      <FleetDetails fleet={fleet} />
      <div className="mt-auto flex flex-col gap-4">
        <FleetIntegrations fleet={fleet} />
        <FleetWaitlist fleet={fleet} />
      </div>
    </Card>
  );
}

function FleetDetails({ fleet }: { fleet: PrebuiltFleet }) {
  const details = [
    { label: "Wakes on", value: fleet.trigger },
    { label: "Delivers", value: fleet.output },
    { label: "Your control", value: fleet.control },
  ];
  return (
    <DescriptionList layout="stacked" className="m-0 border-t border-border pt-4">
      {details.map(({ label, value }) => (
        <div key={label} className="flex flex-col gap-1">
          <DescriptionTerm className="font-sans text-label font-medium text-pulse">{label}</DescriptionTerm>
          <DescriptionDetails className="m-0 font-sans text-body-sm leading-body text-text-muted">{value}</DescriptionDetails>
        </div>
      ))}
    </DescriptionList>
  );
}

function FleetIntegrations({ fleet }: { fleet: PrebuiltFleet }) {
  return (
    <div className="flex flex-wrap items-center gap-2" data-testid={`fleet-integrations-${fleet.id}`}>
      {fleet.integrations.map((integration) => (
        <span key={integration.label} className="inline-flex items-center gap-1.5 rounded-sm border border-border px-2 py-1">
          <img src={integration.icon} alt="" aria-hidden="true" loading="lazy" decoding="async" className="size-4 shrink-0" />
          <span className="font-sans text-label text-text-muted">{integration.label}</span>
        </span>
      ))}
    </div>
  );
}

function FleetWaitlist({ fleet }: { fleet: PrebuiltFleet }) {
  return (
    <Button asChild variant="secondary" className="min-h-11 w-full justify-center">
      <a
        href={WAITLIST_URL}
        target="_blank"
        rel="noopener noreferrer"
        data-testid={`fleet-cta-${fleet.id}`}
        onClick={() =>
          trackSignupStarted({
            source: `fleet_${fleet.id}`,
            surface: "fleets",
            mode: "humans",
          })
        }
      >
        Join the waitlist
      </a>
    </Button>
  );
}
