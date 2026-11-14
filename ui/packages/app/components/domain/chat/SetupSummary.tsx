"use client";

import type { Definition, Review } from "@/lib/api/draft-types";
import { z } from "zod";
import { List, ListItem, Section } from "@agentsfleet/design-system";
import { SetupAccessSummary } from "./SetupAccessSummary";
import { FleetTriggerSchema } from "@/lib/api/fleet-trigger";
import { FleetTriggerSummary } from "@/components/domain/FleetTriggerSummary";
import { SetupQuestions } from "./SetupQuestions";
import { ClockIcon, TargetIcon } from "@phosphor-icons/react/ssr";

const ConfigSchema = z.object({"x-agentsfleet": z.object({triggers: z.array(FleetTriggerSchema).optional()})});
export function SetupSummary({definition, requirements, compact = false}: {definition?: Definition; requirements?: Review["requirements"]; compact?: boolean}) {
  const config = ConfigSchema.safeParse(definition?.config);
  const triggers = config.success ? config.data["x-agentsfleet"].triggers ?? [] : null;
  return <div className="min-w-0 space-y-lg text-label leading-label [overflow-wrap:anywhere]">
    <Section className="gap-sm" aria-label="Fleet purpose">
      <h3 className="flex items-center gap-md text-label font-medium text-muted-foreground"><TargetIcon size={16} aria-hidden="true" />Purpose</h3>
      <p className="break-words text-label leading-label">{definition?.description ?? "Your fleet’s purpose will appear here as you build its setup."}</p>
    </Section>
    <Section className="gap-sm" aria-label="How runs start">
      <h3 className="flex items-center gap-md text-label font-medium text-muted-foreground"><ClockIcon size={16} aria-hidden="true" />When it runs</h3>
      {triggers === null ? <p>Automatic triggers could not be confirmed. Check the fleet’s Setup before relying on automatic runs.</p>
        : triggers.length === 0 ? <p>When you send it a message. No automatic triggers are configured.</p>
        : <><p>When you send it a message, or automatically:</p><List variant="plain" className="space-y-sm text-label leading-label">
          {triggers.map((trigger, index) => <ListItem key={index} className="break-words"><FleetTriggerSummary trigger={trigger} /></ListItem>)}
        </List></>}
    </Section>
    {!compact ? requirements ? <SetupAccessSummary requirements={requirements} /> : <p className="text-muted-foreground">Access and spending limits will appear after the setup is reviewed.</p> : null}
    {definition && !compact ? <SetupQuestions definition={definition} /> : null}
  </div>;
}
