"use client";

import type { Review } from "@/lib/api/draft-types";
import { z } from "zod";
import { List, ListItem, Section } from "@agentsfleet/design-system";
import { ShieldCheckIcon } from "@phosphor-icons/react/ssr";
import { NANOS_PER_USD } from "@/lib/types";

const AmountSchema = z.string().regex(/^[1-9][0-9]*$/);
const NetworkSchema = z.object({
  http_egress: z.enum(["restricted", "public"]).optional(),
  package_manager_domains: z.boolean().optional(),
  allow: z.array(z.string()).optional(), read_only: z.boolean().optional(),
});
export const SetupRequirementsSchema = z.object({
  credentials: z.array(z.string()), repositories: z.array(z.string()), tools: z.array(z.string()),
  network: NetworkSchema.optional(),
  budget: z.object({daily_nanos: AmountSchema, monthly_nanos: AmountSchema.optional()}),
});
const NANO_DIGITS = 9;
const CENT_DIGITS = 2;

export function SetupAccessSummary({requirements}: {requirements: Review["requirements"]}) {
  const parsed = SetupRequirementsSchema.safeParse(requirements);
  if (!parsed.success) return <p className="text-muted-foreground">Access and spending limits could not be summarized. Resolve the setup checks before saving.</p>;
  const {credentials, repositories, tools, network, budget} = parsed.data;
  return <Section className="gap-lg" aria-label="Access and spending limits">
    <h3 className="flex items-center gap-md font-medium text-muted-foreground"><ShieldCheckIcon size={16} className="shrink-0" aria-hidden="true" />Access and spending limits</h3>
    <List variant="plain" className="divide-y divide-border overflow-hidden rounded-lg border border-border text-label leading-label [&>li]:p-lg [&>li>span]:mb-xs [&>li>span]:block [&>li>span]:text-muted-foreground">
      <ListItem><span className="font-medium">Tools: </span>{tools.length ? tools.join(", ") : "No tools requested."}</ListItem>
      <ListItem><span className="font-medium">Secrets and connections: </span>{credentials.length ? credentials.join(", ") : "None requested."}</ListItem>
      <ListItem><span className="font-medium">Repositories: </span>{repositories.length ? repositories.join(", ") : "None requested."}</ListItem>
      <ListItem><span className="font-medium">Internet access: </span>{networkDescription(network)}</ListItem>
      <ListItem><span className="font-medium">Spending limit: </span>{dollars(budget.daily_nanos)} per day{budget.monthly_nanos ? `; ${dollars(budget.monthly_nanos)} per month` : "; no monthly limit"}.</ListItem>
    </List>
    <p className="text-eyebrow leading-eyebrow text-muted-foreground">These are limits, not a fixed charge. Actual spending depends on usage. Secret values are never shown here.</p>
  </Section>;
}

function networkDescription(network: z.infer<typeof NetworkSchema> | undefined): string {
  const readOnly = network?.read_only ? " Read-only policy enabled." : "";
  if (network?.http_egress === "public") return `Public internet access. HTTPS requests to public destinations.${readOnly}`;
  if (network?.package_manager_domains) {
    const additional = network.allow?.length ? ` Additional domains: ${network.allow.join(", ")}.` : "";
    return `Restricted access. Standard package registry domains.${additional}${readOnly}`;
  }
  if (network?.allow === undefined) return "Internet access could not be confirmed. Review the network policy before saving.";
  if (network.allow.length === 0) return "Restricted access. No domains allowed.";
  return `Restricted access. Allowed domains: ${network.allow.join(", ")}.${readOnly}`;
}

function dollars(nanos: string): string {
  const amount = BigInt(nanos);
  const scale = BigInt(NANOS_PER_USD);
  const fraction = String(amount % scale).padStart(NANO_DIGITS, "0").replace(/0+$/, "").padEnd(CENT_DIGITS, "0");
  return `$${amount / scale}.${fraction}`;
}
