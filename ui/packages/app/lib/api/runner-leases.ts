import { z } from "zod";
import { Instant } from "./decode";
import { LEASE_DESTINATION, LEASE_KIND, LEASE_OUTCOME } from "./runners-types";

const count = z.number().int().nonnegative();
const text = z.string().min(1);

// RunnerLeaseItem in afd_api_wire/src/operator.rs. A purged fleet or event
// leaves a retained lease; its authorized destination is an explicit server fact.
const RunnerLeaseSchema = z.object({
  id: text, fleet_id: text, fleet_name: z.string().nullable(), workspace_id: text,
  destination: z.enum(Object.values(LEASE_DESTINATION)),
  event_id: text, event_type: text.nullable(), actor: text.nullable(),
  outcome: z.enum(Object.values(LEASE_OUTCOME)), kind: z.enum(Object.values(LEASE_KIND)),
  failure_label: z.string().nullable(), failure_detail: z.string().nullable(),
  fencing_token: count, provider: text, model: text, posture: text,
  metered_input_tokens: count, metered_cached_tokens: count, metered_output_tokens: count,
  wall_ms: count.nullable(), lease_expires_at: Instant, created_at: Instant,
});
export const RunnerLeasesSchema = z.object({
  items: z.array(RunnerLeaseSchema), total: count.nullable(), next_cursor: z.string().nullable(),
});
export type RunnerLease = z.infer<typeof RunnerLeaseSchema>;
export type RunnerLeaseResponse = z.infer<typeof RunnerLeasesSchema>;
