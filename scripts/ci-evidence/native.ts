import { z } from "zod";
import { identifier, instant, integer } from "./schema.ts";

const text = z.string();
const object = z.record(z.string(), z.json());
const callId = z.string().refine(value => /^-?(?:0|[1-9][0-9]{0,18}):[1-9][0-9]{0,18}$/.test(value)
  && value.split(":").every(part => BigInt(part) >= -(1n << 63n) && BigInt(part) < (1n << 63n)));
export const definitionSchema = z.object({schema_version: z.literal(1), description: text,
  instructions_md: text, config: z.object({name: text, "x-agentsfleet": z.object({model: text, context: z.json().optional()}).passthrough()}).passthrough()}).passthrough();
export const fleetSchema = z.object({id: identifier, setup_revision: text, setup_etag: text,
  policy_semantics: text, definition: definitionSchema}).passthrough();
export const reviewSchema = z.object({review_digest: text, policy_semantics: text,
  effective: z.object({definition: definitionSchema, runtime_model_ref: text}).passthrough()}).passthrough();
export const receiptSchema = z.strictObject({read_url: z.url(), request: z.object({review_digest: text,
  source: z.object({kind: z.literal("draft"), draft_candidate_id: identifier}).optional(),
  draft_candidate_id: identifier.optional()}).passthrough(), response: z.object({id: identifier,
    setup_revision: text, setup_etag: text}).passthrough()});
export const eventSchema = z.object({workspace_id: identifier, fleet_id: identifier, event_id: identifier,
  status: text, request_json: text, response_text: text.nullable(), created_at: instant, updated_at: instant,
  tool_calls: z.object({omitted_call_count: integer, calls: z.array(z.object({call_id: callId, name: text,
    status: z.enum(["succeeded", "failed", "interrupted"]), arguments: z.json().optional(),
    output_head: text.optional(), output_tail: text.optional(), output_line_count: integer.optional()}).passthrough())}).nullable()}).passthrough();
export const requestSchema = z.object({message: text, channel_id: text, reply_thread_ts: text}).passthrough();
export const callSchema = z.strictObject({read_url: z.url(), response: z.object({call_id: callId, arguments: z.json(), output: text,
  output_line_count: integer, truncated: z.boolean(), truncated_arguments: z.boolean()}).passthrough()});
export const settingsSchema = z.strictObject({observed_at: instant,
  catalogue: z.strictObject({read_url: z.url(), response: z.object({models: z.array(z.object({
    id: identifier, provider: text, model_id: text, version: text.nullable().optional(), metadata: z.object({context_cap_tokens: integer}).passthrough(),
    inference: object.nullable().optional()}).passthrough())}).passthrough()}),
  selection: z.strictObject({read_url: z.url(), response: z.object({items: z.array(z.object({
    ref: text, provider: text, model: text, context_window_tokens: integer, max_output_tokens: integer}).passthrough())}).passthrough()})});
export const identitySchema = z.strictObject({captured_at: instant, workspace_id: identifier, fleet_id: identifier, event_id: identifier,
  leases: z.array(z.strictObject({lease_id: identifier, runner_id: identifier, provider: text, model: text,
    status: text, created_at: instant, updated_at: instant})).min(1)});
export const freezeSchema = z.strictObject({operator: text.min(1), agreed_at: instant, at: instant,
  evaluation_id: text, model_record_id: identifier, no_changes_during_campaign: z.literal(true)});

export function eventUrl(workspace: string, fleet: string, event: string): string {
  return `https://api-dev.agentsfleet.net/v1/workspaces/${workspace}/fleets/${fleet}/events/${event}`;
}
