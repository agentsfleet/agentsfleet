// Native setup is validated by the daemon; browser reads are parsed independently.

import { z } from "zod";
import { Instant } from "./decode";

import { DRAFT_STATE, MESSAGE_ROLE } from "./draft-vocabulary";
export { DRAFT_STATE, MESSAGE_ROLE } from "./draft-vocabulary";
export const MESSAGE_STATE = {
  queued: "queued", running: "running", completed: "completed", cancelled: "cancelled",
  failed: "failed", timedOut: "timed_out",
} as const;
export const SOURCE_KIND = {blank: "blank", definition: "definition", library: "library", fleet: "fleet"} as const;
export const START_KIND = {none: "none", runNow: "run_now"} as const;
/** `SyncStatus` in rustd/crates/afd_cron/src/model.rs; only `synced` means the external scheduler will fire it. */
export const SCHEDULE_SYNC = {syncing: "syncing", synced: "synced", failed: "failed"} as const;
export const SETUP_HEADERS = {match: "If-Match", key: "Idempotency-Key"} as const;
export const MESSAGE_MAX_BYTES = 32 * 1024;
export const INSTRUCTIONS_MAX_BYTES = 32 * 1024;
export const PAGE_LIMIT = 25;
// The largest page a workspace model or private-chat list answers with.
export const LIST_PAGE_MAX = 100;
const DECIMAL = /^(0|[1-9][0-9]*)$/;
const decimal = z.string().regex(DECIMAL);
const encoder = new TextEncoder();
export const MessageInputSchema = z.string().min(1).refine((text) => encoder.encode(text).byteLength <= MESSAGE_MAX_BYTES);
const record = z.record(z.string(), z.unknown());

export const DefinitionSchema = z.object({
  schema_version: z.literal(1), description: z.string().min(1), instructions_md: z.string(), config: record,
});
export const CandidateSchema = z.object({
  id: z.uuidv7(), revision: decimal, definition: DefinitionSchema,
  canonical_digest: z.string(), policy_semantics: z.string(),
});
export const CheckSchema = z.object({
  kind: z.string(), subject: z.string(), state: z.string(),
  next_action: z.object({kind: z.string(), label: z.string().optional(), href: z.string().optional()}).optional(),
});
export const ReviewSchema = z.object({
  effective: z.object({
    definition: DefinitionSchema, instance_name: z.string(), runtime_model_ref: z.string().nullable(),
    source_revision: z.string().optional(), target_setup_revision: decimal.optional(),
  }),
  requirements: record, readiness: z.array(CheckSchema), review_digest: z.string(),
  policy_semantics: z.string(), observed_at: Instant,
});
export const AcceptedSchema = z.object({status: z.literal("accepted"), sequence: decimal, replayed: z.boolean()});
export const CancellationSchema = z.object({cancelled_through: decimal,
  stopped: z.array(z.object({sequence: decimal, state: z.literal(MESSAGE_STATE.cancelled)}))});
export const DraftSchema = z.object({
  id: z.uuidv7(), state: z.enum(DRAFT_STATE), title: z.string(), etag: z.string(),
  authoring_model_ref: z.string(), cancelled_through: decimal,
  candidate: CandidateSchema.optional(), review: ReviewSchema.optional(), linked_fleet_id: z.uuidv7().optional(),
  publication: z.object({library_id: z.string(), revision_id: z.uuidv7(), canonical_digest: z.string()}).optional(),
});
export const CreatedMessageSchema = DraftSchema.extend({accepted: AcceptedSchema});
export const MessageSchema = z.object({
  sequence: decimal, operation_id: z.uuidv7(), role: z.enum(MESSAGE_ROLE), state: z.enum(MESSAGE_STATE),
  text: z.string().optional(), created_at: Instant, candidate_id: z.uuidv7().optional(),
  failure: z.object({reason: z.string(), error_code: z.string(), recovery_action: z.string()}).optional(),
});
export const MessagesSchema = z.object({items: z.array(MessageSchema).max(PAGE_LIMIT), next_cursor: z.string().nullable()});
// The published schedule `View`; sync stays an open string so an unknown state counts as not registered.
export const ScheduleSchema = z.object({
  schedule_id: z.string(), fleet_id: z.string(), sync: z.string(), last_error: z.string().nullable().optional(),
});
export const FleetSchema = z.object({
  id: z.uuidv7(), name: z.string(), status: z.string(), etag: z.string(), setup_etag: z.string(), setup_revision: decimal,
  webhook_urls: z.array(z.object({source: z.string(), url: z.url()})),
  definition: DefinitionSchema.optional(), setup: z.object({state: z.string(), checks: z.array(CheckSchema)}),
  start: z.discriminatedUnion("kind", [z.object({kind: z.literal(START_KIND.none)}), z.object({
    kind: z.literal(START_KIND.runNow), state: z.enum(["pending", "accepted", "blocked"]),
    event_id: z.string().optional(), error_code: z.string().optional(),
  })]),
  schedules: z.array(ScheduleSchema),
});
export const PublishedSchema = z.object({
  library_id: z.string(), revision_id: z.uuidv7(), head_revision_id: z.uuidv7(), etag: z.string(),
});
export const AppliedSchema = z.object({fleet_id: z.uuidv7(), setup_revision: decimal, setup_etag: z.string(), etag: z.string(),
  schedules: z.array(ScheduleSchema)});
export const ModelsSchema = z.object({items: z.array(z.object({
  ref: z.string(), label: z.string(), provider: z.string(), model: z.string(),
  context_window_tokens: z.number().int().positive(), max_output_tokens: z.number().int().positive().optional(),
  eligibility: z.object({authoring: z.boolean(), runtime: z.boolean()}),
  pricing: z.object({input_nanos_per_million: decimal, output_nanos_per_million: decimal, runtime_nanos_per_second: decimal}),
})).max(LIST_PAGE_MAX), next_cursor: z.string().nullable()});
export const DraftCardsSchema = z.object({items: z.array(z.object({
  id: z.uuidv7(), title: z.string(), state: z.enum(DRAFT_STATE), etag: z.string(),
  linked_fleet_id: z.uuidv7().nullable().default(null), updated_at: Instant, expires_at: Instant.nullable().default(null),
})).max(LIST_PAGE_MAX), next_cursor: z.string().nullable()});

export type Definition = z.infer<typeof DefinitionSchema>;
export type Draft = z.infer<typeof DraftSchema>;
export type Review = z.infer<typeof ReviewSchema>;
export type Message = z.infer<typeof MessageSchema>;
export type Fleet = z.infer<typeof FleetSchema>;
export type Schedule = z.infer<typeof ScheduleSchema>;
export type WorkspaceModel = z.infer<typeof ModelsSchema>["items"][number];
export type DraftCard = z.infer<typeof DraftCardsSchema>["items"][number];
export type Source = {kind: typeof SOURCE_KIND.blank} | {kind: typeof SOURCE_KIND.definition; definition: Definition}
  | {kind: typeof SOURCE_KIND.library; library_id: string; revision_id: string}
  | {kind: typeof SOURCE_KIND.fleet; fleet_id: string; setup_revision: string};
export type Start = {kind: typeof START_KIND.none} | {kind: typeof START_KIND.runNow; message: string};

/** A schedule the scheduler has not confirmed: it may never fire until its registration is retried. */
export function scheduleOwed(schedule: Schedule): boolean { return schedule.sync !== SCHEDULE_SYNC.synced; }

/** What Apply reports: success only when every schedule it returned is registered. */
export function appliedNotice(schedules: readonly Schedule[]): string {
  return schedules.some(scheduleOwed) ? "Fleet setup updated, but a schedule is not registered yet."
    : "Fleet setup updated. New work uses this setup revision.";
}

export function isRunning(message: Message): boolean {
  return message.state === MESSAGE_STATE.queued || message.state === MESSAGE_STATE.running;
}
