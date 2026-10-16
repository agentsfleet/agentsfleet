import type { Draft, Fleet, Message, Review, WorkspaceModel } from "@/lib/api/draft-types";
import { DRAFT_STATE, MESSAGE_ROLE, MESSAGE_STATE, START_KIND } from "@/lib/api/draft-types";
import { NANOS_PER_USD } from "@/lib/types";

export const WORKSPACE = "0199a801-1000-7000-8000-000000000001";
export const DRAFT_ID = "0199a801-1000-7000-8000-000000000002";
export const CANDIDATE_ID = "0199a801-1000-7000-8000-000000000003";
export const FLEET_ID = "0199a801-1000-7000-8000-000000000004";
export const OPERATION_ID = "0199a801-1000-7000-8000-000000000005";
export const LIBRARY_ID = "0199a801-1000-7000-8000-000000000006";
export const REVISION_ID = "0199a801-1000-7000-8000-000000000007";
export const APP_ORIGIN = "https://app.example.test";
export const MODEL_REF = "model:authoring";
export const REVIEW: Review = {effective: {definition: {schema_version: 1,
  description: "Investigate failures", instructions_md: "Investigate failures.\n",
  config: {name: "repairer", "x-agentsfleet": {triggers: [], tools: [], budget: {daily_nanos: String(NANOS_PER_USD)}}}},
  instance_name: "repairer", runtime_model_ref: MODEL_REF}, requirements: {}, readiness: [], review_digest: "review-one", policy_semantics: "native-setup-1", observed_at: 0};
export const DRAFT: Draft = {id: DRAFT_ID, state: DRAFT_STATE.editing, title: "Investigate failures", etag: '"draft-one"',
  authoring_model_ref: MODEL_REF, cancelled_through: "0", review: REVIEW,
  candidate: {id: CANDIDATE_ID, revision: "1", definition: REVIEW.effective.definition, canonical_digest: "candidate-one", policy_semantics: REVIEW.policy_semantics}};
export const FLEET: Fleet = {id: FLEET_ID, name: "repairer", status: "installing", etag: '"fleet-one"', setup_etag: '"setup-one"',
  setup_revision: "1", webhook_urls: [], setup: {state: "waiting", checks: [{kind: "credentials", subject: "github", state: "blocked",
    next_action: {kind: "workspace_connections", label: "Connect GitHub", href: `/w/${WORKSPACE}/integrations`}}]}, start: {kind: START_KIND.none}};
export const FLEET_DETAIL = {...FLEET, definition: REVIEW.effective.definition, created_at: 0, updated_at: 0,
  policy_semantics: REVIEW.policy_semantics, needs_re_review: false, events_processed: "3", budget_used_nanos: "9007199254740993",
  pending_approvals: 2, memory_access: {read: true, publish: false}};
export const MESSAGE: Message = {sequence: "1", operation_id: OPERATION_ID, role: MESSAGE_ROLE.user,
  state: MESSAGE_STATE.completed, text: "Investigate failures", created_at: 0};
export const MODEL: WorkspaceModel = {ref: MODEL_REF, label: "Authoring model", provider: "openai", model: "gpt-4o",
  context_window_tokens: 128000, max_output_tokens: 8192, eligibility: {authoring: true, runtime: true},
  pricing: {input_nanos_per_million: String(2.5 * NANOS_PER_USD), output_nanos_per_million: String(10 * NANOS_PER_USD), runtime_nanos_per_second: "1"}};
export const ACCEPTED = {status: "accepted", sequence: "1", replayed: false} as const;
export const PUBLICATION = {library_id: LIBRARY_ID, revision_id: REVISION_ID, head_revision_id: REVISION_ID, etag: '"library-one"'};
export const LIBRARY_CARD = {id: "checkout-watch", name: "Checkout watch", description: "Investigate failures", visibility: "platform" as const,
  head_revision_id: REVISION_ID, etag: PUBLICATION.etag, requirements: {}, created_at: 0, required_credentials_reasons: {}};
export const LIBRARY = {card: LIBRARY_CARD, head: {id: REVISION_ID, library_id: LIBRARY_CARD.id, revision: "1",
  definition: REVIEW.effective.definition, canonical_digest: "library-one"}, review: REVIEW, etag: PUBLICATION.etag};

export function response(value: unknown, status = 200, headers?: HeadersInit): Response {
  return new Response(status === 204 ? null : JSON.stringify(value), {status, headers: {"Content-Type": "application/json", ...headers}});
}
