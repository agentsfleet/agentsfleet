import { z } from "zod";
import { Instant } from "./decode";
import { nanosSchema } from "./nanos";
import { ScheduleSchema } from "./schedule-types";

/** A fleet's network: private by default, and only a signed-in person opens it. */
export const HTTP_EGRESS = {restricted: "restricted", public: "public"} as const;
/** `afd_api_wire::grant::status`: a deny is stored as revoked. */
export const GRANT_STATUS = {pending: "pending", approved: "approved", revoked: "revoked"} as const;
/** The grant name a GitHub request carries; its scope names repositories and a reach. */
export const GITHUB_GRANT = "github";
export const TRIGGER_KIND = {webhook: "webhook", appEvent: "app_event", mention: "mention"} as const;

const DecimalSchema = z.string().regex(/^(0|[1-9][0-9]*)$/);
// REST counters are decimal strings. Decode to bigint because nanos can
// exceed Number.MAX_SAFE_INTEGER and must survive rendering without rounding.
const CounterSchema = DecimalSchema.transform((value) => BigInt(value));

/** One row that lets something outside wake a fleet (`GET F/triggers`). */
export const TriggerSchema = z.object({
  id: z.uuidv7(), kind: z.string(), source: z.string(),
  events: z.array(z.string()).nullish(), repositories: z.array(z.string()).nullish(),
  channel: z.string().nullish(), url: z.url().nullish(), created_at: Instant,
});
/** One credential a person approved, or was asked for (`GET F/integration-grants`). */
export const GrantSchema = z.object({
  id: z.uuidv7(), kind: z.string(), name: z.string().min(1), status: z.string(),
  scope: z.unknown().optional(), requested_scope: z.unknown().optional(), approved_at: Instant.nullish(),
});
/** The GitHub scope a grant carries: which repositories, and how far. */
const GithubScopeSchema = z.object({repositories: z.array(z.string().min(1)).min(1), access: z.enum(["read", "write"])});

export const FleetSummarySchema = z.object({
  id: z.string(), name: z.string(), status: z.string(),
  created_at: Instant, updated_at: Instant,
  events_processed: CounterSchema, budget_used_nanos: nanosSchema.transform((value) => BigInt(value)),
});
export const FleetListSchema = z.object({
  items: z.array(FleetSummarySchema), total: z.null(), next_cursor: z.string().nullable(),
});
export const FleetDetailSchema = z.object({
  id: z.uuidv7(), name: z.string(), status: z.string(), etag: z.string(),
  model_ref: z.string().min(1), http_egress: z.enum([HTTP_EGRESS.restricted, HTTP_EGRESS.public]), package_domains: z.boolean(),
  schedules: z.array(ScheduleSchema), triggers: z.array(TriggerSchema), grants: z.array(GrantSchema),
  memory_access: z.object({read: z.boolean(), publish: z.boolean()}),
  created_at: Instant, updated_at: Instant,
  events_processed: DecimalSchema, budget_used_nanos: nanosSchema,
  pending_approvals: z.number().int().nonnegative(),
});

export type NativeFleetSummary = z.infer<typeof FleetSummarySchema>;
export type NativeFleetDetail = z.infer<typeof FleetDetailSchema>;
export type FleetTrigger = z.infer<typeof TriggerSchema>;
export type FleetGrant = z.infer<typeof GrantSchema>;
export type HttpEgress = NativeFleetDetail["http_egress"];
export type GithubScope = z.infer<typeof GithubScopeSchema>;

/** A GitHub scope, or null when the value names no repositories or reach. */
export function githubScope(value: unknown): GithubScope | null {
  const parsed = GithubScopeSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}
