import { z } from "zod";
import { Instant } from "./decode";
import { nanosSchema } from "./nanos";

// Optional telemetry stays unknown; it must never become an invented zero.
const Count = z.number().int().nullable().default(null);
const Text = z.string().nullable().default(null);
const EventRowSchema = z.object({
  event_id: z.string(), fleet_id: z.string(), workspace_id: z.string(), actor: z.string(),
  event_type: z.string(), status: z.string(),
  tokens: Count, wall_ms: Count, cost_nanos: nanosSchema.nullable().default(null),
  failure_label: Text, failure_detail: Text, resumes_event_id: Text,
  created_at: Instant, updated_at: Instant,
});

export const EventsPageSchema = z.object({items: z.array(EventRowSchema), next_cursor: z.string().nullable()});
export type EventRow = z.infer<typeof EventRowSchema>;
export type EventsPage = z.infer<typeof EventsPageSchema>;
