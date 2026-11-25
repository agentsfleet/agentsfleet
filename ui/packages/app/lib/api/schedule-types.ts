// Schedule rows and their vocabulary, free of transport so navigation and
// client components read them without loading a request module.

import { z } from "zod";
import { Instant } from "./decode";

/** `SyncStatus` in rustd/crates/daemon/afd_cron/src/model.rs; only `synced` means the external scheduler will fire it. */
export const SCHEDULE_SYNC = {syncing: "syncing", synced: "synced", failed: "failed"} as const;
export const SCHEDULE_RUN_STATE = {
  scheduled: "scheduled", paused: "paused", activationPending: "activation_pending", unavailable: "next_run_unavailable",
} as const;

// The daemon's schedule `View`; sync stays an open string so an unknown state counts as not registered.
export const ScheduleSchema = z.object({
  schedule_id: z.string(), fleet_id: z.string(), sync: z.string(), last_error: z.string().nullable().optional(),
  cron: z.string(), timezone: z.string(), message: z.string(), status: z.string(),
  description: z.string().min(1), run_state: z.string(), next_run_at: Instant.nullable(),
});

export type Schedule = z.infer<typeof ScheduleSchema>;
