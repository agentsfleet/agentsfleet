import { z } from "zod";
import type { Definition, Review } from "./api/draft-types";
import { FleetTriggerSchema } from "./api/fleet-trigger";
import { AGENT_TRIGGER_TYPE } from "./types";

const AmountSchema = z.string().regex(/^[1-9][0-9]*$/);
export const SetupRequirementsSchema = z.object({
  credentials: z.array(z.string()), repositories: z.array(z.string()), tools: z.array(z.string()),
  network: z.object({http_egress: z.enum(["restricted", "public"]).optional(),
    package_manager_domains: z.boolean().optional(), allow: z.array(z.string()).optional(), read_only: z.boolean().optional()}).optional(),
  budget: z.object({daily_nanos: AmountSchema, monthly_nanos: AmountSchema.optional()}),
});
const SavedConfigSchema = SetupRequirementsSchema.extend({
  credentials: z.array(z.string()).nullish().transform(names => names ?? []),
  repositories: z.array(z.string()).nullish().transform(names => names ?? []),
  network: SetupRequirementsSchema.shape.network.nullable().transform(network => network ?? undefined),
  model: z.string().optional(),
});
const TimingSchema = z.object({triggers: z.array(FleetTriggerSchema).default([])});

export type SetupNetworkChange = {http_egress: "public" | "restricted"} | {package_manager_domains: boolean};

export function savedSetupRequirements(definition?: Definition) {
  const result = SavedConfigSchema.safeParse(definition?.config["x-agentsfleet"]);
  return result.success ? result.data : null;
}

/** Review requirements are authoritative. Saved definitions can omit optional fields. */
export function setupRequirements(definition?: Definition, requirements?: Review["requirements"]) {
  if (requirements === undefined) return savedSetupRequirements(definition);
  const result = SetupRequirementsSchema.safeParse(requirements);
  return result.success ? result.data : null;
}

export function hasProposedWakeup(definition?: Definition): boolean {
  const timing = TimingSchema.safeParse(definition?.config["x-agentsfleet"]);
  return timing.success && timing.data.triggers.some(trigger => trigger.type === AGENT_TRIGGER_TYPE.cron);
}
