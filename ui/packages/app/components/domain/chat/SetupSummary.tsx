"use client";

import type { Definition, Review } from "@/lib/api/draft-types";
import { z } from "zod";
import { List, ListItem, Section } from "@agentsfleet/design-system";
import { NANOS_PER_USD } from "@/lib/types";
import { FleetTriggerSchema } from "@/lib/api/fleet-trigger";
import { FleetTriggerSummary } from "@/components/domain/FleetTriggerSummary";

const ConfigSchema = z.object({"x-agentsfleet": z.object({triggers: z.array(FleetTriggerSchema).optional()})});
const AmountSchema = z.string().regex(/^[1-9][0-9]*$/);
const RequirementsSchema = z.object({
  credentials: z.array(z.string()), repositories: z.array(z.string()), tools: z.array(z.string()),
  network: z.object({allow: z.array(z.string()).optional(), read_only: z.boolean().optional()}).optional(),
  budget: z.object({daily_nanos: AmountSchema, monthly_nanos: AmountSchema.optional()}),
});
const NANO_DIGITS = 9;
const CENT_DIGITS = 2;

export function SetupSummary({definition, requirements}: {definition?: Definition; requirements?: Review["requirements"]}) {
  const config = ConfigSchema.safeParse(definition?.config);
  const triggers = config.success ? config.data["x-agentsfleet"].triggers ?? [] : null;
  return <div className="space-y-lg">
    <Section className="gap-sm" aria-label="Fleet purpose">
      <h3 className="text-sm font-semibold text-primary">What this fleet does</h3>
      <p className="break-words text-heading font-semibold">{definition?.description ?? "Open the fleet to see its saved instructions."}</p>
    </Section>
    <Section className="gap-sm" aria-label="How runs start">
      <h3 className="font-semibold">When it runs</h3>
      {triggers === null ? <p className="text-sm">Automatic triggers could not be confirmed. Check the fleet’s Setup before relying on automatic runs.</p>
        : triggers.length === 0 ? <p className="text-sm">When you send it a message. No automatic triggers are configured.</p>
        : <><p className="text-sm">When you send it a message, or automatically:</p><List variant="plain" className="space-y-sm">
          {triggers.map((trigger, index) => <ListItem key={index} className="break-words text-sm"><FleetTriggerSummary trigger={trigger} /></ListItem>)}
        </List></>}
    </Section>
    {requirements ? <AccessSummary requirements={requirements} /> : null}
  </div>;
}

function AccessSummary({requirements}: {requirements: Review["requirements"]}) {
  const parsed = RequirementsSchema.safeParse(requirements);
  if (!parsed.success) return <p className="text-sm text-muted-foreground">Access and spending limits could not be summarized. Resolve the setup checks before saving.</p>;
  const {credentials, repositories, tools, network, budget} = parsed.data;
  return <Section className="gap-sm" aria-label="Access and spending limits">
    <h3 className="font-semibold">Access and spending limits</h3>
    <List variant="plain" className="space-y-sm text-sm">
      <ListItem><span className="font-medium">Tools: </span>{tools.length ? tools.join(", ") : "No tools requested."}</ListItem>
      <ListItem><span className="font-medium">Connected accounts: </span>{credentials.length ? credentials.join(", ") : "None requested."}</ListItem>
      <ListItem><span className="font-medium">Repositories: </span>{repositories.length ? repositories.join(", ") : "None requested."}</ListItem>
      <ListItem><span className="font-medium">Network: </span>{network?.allow === undefined ? "No explicit host limit shown here. Confirm the network policy before saving."
        : network.allow.length ? `Requests to ${network.allow.join(", ")}.${network.read_only ? " Read-only policy enabled." : ""}` : "Outbound requests are blocked."}</ListItem>
      <ListItem><span className="font-medium">Spending limit: </span>{dollars(budget.daily_nanos)} per day{budget.monthly_nanos ? `; ${dollars(budget.monthly_nanos)} per month` : "; no monthly limit"}.</ListItem>
    </List>
    <p className="text-xs text-muted-foreground">These are limits, not a fixed charge. Actual spending depends on usage.</p>
  </Section>;
}

function dollars(nanos: string): string {
  const amount = BigInt(nanos);
  const scale = BigInt(NANOS_PER_USD);
  const fraction = String(amount % scale).padStart(NANO_DIGITS, "0").replace(/0+$/, "").padEnd(CENT_DIGITS, "0");
  return `$${amount / scale}.${fraction}`;
}
