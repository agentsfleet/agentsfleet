import { useId } from "react";
import { DescriptionDetails, DescriptionList, DescriptionTerm, DisplayLG, Section, cn } from "@agentsfleet/design-system";
import { LOOP_ANCHOR_ID } from "../lib/marketing-copy";
import { linkTracking } from "../analytics/link-tracking";
import { SECTION_SPACING, SITE_CONTAINER, TEXT_LINK } from "./site-classes";

const FLEET_LISTS = [
  ["Wakes on", "See the schedules and triggers the fleet added. You can remove any of them."],
  ["Can access", "See the secrets and connections you approved. You can remove access from the fleet page."],
  ["Remembers", "Ask the fleet to keep standing instructions. The fleet uses them on later runs."],
] as const;

export function AudienceSection() {
  const heading = useId();
  return (
    <Section variant="divided" className={SECTION_SPACING} aria-labelledby={heading} data-testid="audience-section">
      <div className={cn(SITE_CONTAINER, "grid gap-2xl lg:grid-cols-[1fr_2fr] lg:gap-5xl")}>
        <div className="grid content-start gap-lg">
          <DisplayLG id={heading} className="font-title">What you can see in a fleet</DisplayLG>
          <a
            href={`/#${LOOP_ANCHOR_ID}`}
            className={cn(TEXT_LINK, "w-fit font-semibold text-pulse")}
            {...linkTracking("continuity_product", "continuity", "product")}
          >
            Read how fleets work
          </a>
        </div>
        <DescriptionList layout="stacked" className="grid space-y-0 gap-xl sm:grid-cols-3 sm:gap-2xl">
          {FLEET_LISTS.map(([title, description]) => (
            <div key={title} className="grid content-start gap-md border-t border-border pt-lg">
              <DescriptionTerm className="text-heading font-title text-text">{title}</DescriptionTerm>
              <DescriptionDetails className="text-conversation leading-conversation text-text-muted">{description}</DescriptionDetails>
            </div>
          ))}
        </DescriptionList>
      </div>
    </Section>
  );
}
