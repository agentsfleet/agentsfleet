import { z } from "zod";
import { Instant } from "@/lib/api/decode";
import { RunDestination, RunKind, RunOutcome } from "./runner-control-types";

const count = z.number().int().nonnegative();
const text = z.string().min(1);

// RunnerRunItem in afd_api_wire/src/operator.rs. A purged fleet or turn
// leaves a retained run; its authorized destination is an explicit server fact.
const RunnerRunSchema = z.object({
  id: text, fleet_id: text, fleet_name: z.string().nullable(), workspace_id: text,
  destination: z.enum(Object.values(RunDestination)),
  turn_id: text, event_type: text.nullable(), actor: text.nullable(),
  outcome: z.enum(Object.values(RunOutcome)), kind: z.enum(Object.values(RunKind)),
  failure_label: z.string().nullable(), failure_detail: z.string().nullable(),
  fencing_token: count, provider: text, model: text, posture: text,
  metered_input_tokens: count, metered_cached_tokens: count, metered_output_tokens: count,
  wall_ms: count.nullable(), run_expires_at: Instant, created_at: Instant,
});
export const RunnerRunsSchema = z.object({
  items: z.array(RunnerRunSchema), total: count.nullable(), next_cursor: z.string().nullable(),
});
export type RunnerRun = z.infer<typeof RunnerRunSchema>;
export type RunnerRunResponse = z.infer<typeof RunnerRunsSchema>;
