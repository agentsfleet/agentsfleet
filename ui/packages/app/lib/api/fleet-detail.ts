import { z } from "zod";
import { AGENT_TRIGGER_TYPE } from "@/lib/types";
import { DefinitionSchema, FleetSchema } from "./draft-types";

const TriggerSchema = z.discriminatedUnion("type", [
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.cron), schedule: z.string()}),
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.webhook), source: z.string(), events: z.array(z.string()).optional()}),
  z.object({type: z.literal(AGENT_TRIGGER_TYPE.mention), source: z.string(), channels: z.array(z.string())}),
]);
const ConfigSchema = z.object({"x-agentsfleet": z.object({triggers: z.array(TriggerSchema).optional()}).passthrough()});
const DecimalSchema = z.string().regex(/^(0|[1-9][0-9]*)$/);

// REST counters are decimal strings. Decode to bigint because nanos can
// exceed Number.MAX_SAFE_INTEGER and must survive rendering without rounding.
const CounterSchema = DecimalSchema.transform((value) => BigInt(value));
export const FleetSummarySchema = z.object({
  id: z.string(), name: z.string(), status: z.string(),
  created_at: z.number().int(), updated_at: z.number().int(),
  triggers: z.array(TriggerSchema).optional(),
  events_processed: CounterSchema, budget_used_nanos: CounterSchema,
});
export const FleetListSchema = z.object({
  items: z.array(FleetSummarySchema), total: z.null(), next_cursor: z.string().nullable(),
});
export type NativeFleetSummary = z.infer<typeof FleetSummarySchema>;

export const FleetDetailSchema = FleetSchema.extend({
  definition: DefinitionSchema.extend({config: ConfigSchema.passthrough()}),
  created_at: z.number().int(), updated_at: z.number().int(),
  policy_semantics: z.string(), needs_re_review: z.boolean(),
  events_processed: DecimalSchema, budget_used_nanos: DecimalSchema,
  pending_approvals: z.number().int().nonnegative(),
  memory_access: z.object({read: z.boolean(), publish: z.boolean()}),
}).transform((fleet) => ({...fleet, triggers: fleet.definition.config["x-agentsfleet"].triggers ?? []}));

export type NativeFleetDetail = z.infer<typeof FleetDetailSchema>;
