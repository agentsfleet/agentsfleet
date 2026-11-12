import type {ActionResult} from "@/lib/actions/with-token";
import type {MemberSummary} from "@/lib/api/tenant-members";
import type {InviteSummary} from "@/lib/api/invites";
import type {Secret} from "@/lib/api/secrets";
import type {TenantModelEntryPageResult} from "@/lib/api/tenant_model_entries";
import {CATALOGUE_MODEL} from "@/tests/helpers/catalogue-fixtures";
import {WORKSPACE, FLEET_ID} from "@/tests/helpers/draft-fixtures";

export const SAMPLE_MEMBERS: MemberSummary[] = [
  {user_id: "fixture-owner", display_name: "Kishore Neelamegam", email: "you@example.test", role: "owner", joined_at: "2026-11-01T10:00:00Z"},
  {user_id: "member", display_name: "Release reviewer", email: "reviewer@example.test", role: "member", joined_at: "2026-11-05T10:00:00Z"},
];
export const SAMPLE_INVITES: InviteSummary[] = [{id: "invite", email: "teammate@example.test", role: "member", created_at: "2026-11-11T10:00:00Z",
  expires_at: "2026-11-18T10:00:00Z", link: "https://example.test/invitation", email_status: "failed", email_sent_at: null}];
const SECRETS: Secret[] = [{kind: "provider_key", provider: CATALOGUE_MODEL.provider, name: "Fireworks workspace", created_at: "2026-11-11T10:00:00Z"}];
export const SAMPLE_MODELS: TenantModelEntryPageResult = {models: [{id: "model", model_id: CATALOGUE_MODEL.id,
  display_name: CATALOGUE_MODEL.display_name, workspace_id: WORKSPACE, secret_ref: SECRETS[0]?.name ?? "", provider: CATALOGUE_MODEL.provider,
  kind: "provider_key", active: true, has_key: true, context_cap_tokens: CATALOGUE_MODEL.context_cap_tokens, created_at: "2026-11-11T10:00:00Z"}],
  platform_default_available: true, platform_default: {context_cap_tokens: CATALOGUE_MODEL.context_cap_tokens, provider: "fireworks", model: CATALOGUE_MODEL.id, display_name: CATALOGUE_MODEL.display_name}, next_cursor: null, total: 1};

/** Visual reads have sample data; a fixture can never send a real mutation. */
export async function getModelLibraryAction() {return {ok: true, data: {version: "sample", models: [CATALOGUE_MODEL], total: 1, next_cursor: null}};}
export async function listSecretsAction() {return {ok: true, data: {secrets: SECRETS}};}
export async function listModelEntriesAction() {return {ok: true, data: SAMPLE_MODELS};}
export async function loadTeamAction() {return {ok: true, data: {members: SAMPLE_MEMBERS, invites: SAMPLE_INVITES}};}
export async function listApprovalsAction() {return {ok: true, data: {items: new URLSearchParams(location.search).get("state") === "completed" ? [{gate_id: "sample-release-approval", workspace_id: WORKSPACE, fleet_id: FLEET_ID,
  status: "pending", proposed_action: "Publish runner image", tool_name: "registry", action_name: "publish", blast_radius: "Production registry · Release tag"}] : [], next_cursor: null}};}
async function refuse(): Promise<ActionResult<never>> {return {ok: false, status: 503, error: "Read-only component fixture. No changes were sent."};}
export const resetProviderAction = refuse;
export const approveApprovalAction = refuse;
export const denyApprovalAction = refuse;
export const setProviderSelfManagedAction = refuse;
export const deleteModelEntryAction = refuse;
export const createModelEntryAction = refuse;
export const updateModelEntryAction = refuse;
export const findRegisteredModelAction = refuse;
export const readModelActivationAction = refuse;
export const replaceSecretAction = refuse;
export const createSecretAction = refuse;
export const removeMemberAction = refuse;
export const revokeInviteAction = refuse;
export const sendInviteEmailAction = refuse;
export const createInviteAction = refuse;
export const deleteSecretAction = refuse;
export const createApiKeyAction = refuse;
export const listApiKeysAction = refuse;
export const revokeApiKeyAction = refuse;
export const deleteApiKeyAction = refuse;
export const startConnectAction = refuse;
export const disconnectConnectorAction = refuse;
