import {
  Button,
  Card,
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

/*
 * Prebuilt fleets — the droids-style "ready to run" wall. Each card is
 * category · name · description · connected-app logos · a waitlist CTA. The
 * three behavioral pillars (isolated / compounding / proactive) moved down to
 * the Core Capabilities section, so this stays a clean catalogue. Keeps
 * id={LOOP_ANCHOR_ID} so the hero "Meet the fleet" link, the footer "fleets"
 * link, nav, and the llms.txt anchor all still resolve.
 */
export default function PrebuiltFleets() {
  return (
    <Section asChild className="site-section" data-testid="prebuilt-fleets">
      <section id={LOOP_ANCHOR_ID} aria-label="Prebuilt fleets, ready to run">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <SectionLabel className="mb-0">the fleet · ready to run</SectionLabel>
            <DisplayLG>{FLEETS_SECTION_HEADING}</DisplayLG>
            <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">
              {FLEETS_SECTION_LEDE}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {PREBUILT_FLEETS.map((fleet) => (
              <FleetCard key={fleet.id} fleet={fleet} />
            ))}
            <ComingSoonCard />
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
        <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
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
      <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
        {fleet.name}
      </h3>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        {fleet.description}
      </p>

      <div className="mt-auto flex flex-col gap-4">
        <div
          className="flex flex-wrap items-center gap-2"
          data-testid={`fleet-integrations-${fleet.id}`}
        >
          {fleet.integrations.map((integration) => (
            <span
              key={integration.label}
              className="inline-flex items-center gap-1.5 rounded-sm border border-border px-2 py-1"
            >
              <img
                src={integration.icon}
                alt=""
                aria-hidden="true"
                loading="lazy"
                decoding="async"
                className="size-4 shrink-0"
              />
              <span className="font-mono text-label text-text-muted">
                {integration.label}
              </span>
            </span>
          ))}
        </div>

        <Button
          asChild
          variant="secondary"
          className="min-h-11 w-full justify-center"
        >
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
            {fleet.comingSoon ? "Join the waitlist" : "Try it"}
          </a>
        </Button>
      </div>
    </Card>
  );
}

function ComingSoonCard() {
  return (
    <div
      className="flex h-full min-h-[12rem] flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border p-2xl text-center"
      data-testid="fleet-card-coming-soon"
    >
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        more
      </span>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        More prebuilt fleets are joining the fleet — coming soon.
      </p>
    </div>
  );
}
