import type { MemberSummary } from "@/lib/api/tenant-members";
import type { InviteSummary } from "@/lib/api/invites";
import type { TenantModelEntryPageResult } from "@/lib/api/tenant_model_entries";
import { CATALOGUE_MODEL } from "@/tests/helpers/catalogue-fixtures";

export const LONG_SECRET_NAMES = {
  unbroken: "billing-warehouse-replica-read-only-credentials-for-the-nightly-export",
  spaced: "Release registry credentials for staging",
} as const;

/** Only read-only browser fixtures read this switch; production data is untouched. */
export function visualParity() { return new URLSearchParams(location.search).has("parity"); }

export function visualSecretName() {
  const name = new URLSearchParams(location.search).get("longSecret");
  return name === "spaced" ? LONG_SECRET_NAMES.spaced : name !== null ? LONG_SECRET_NAMES.unbroken
    : visualParity() ? "REGISTRY_TOKEN" : "Release service";
}

export function visualMembers(sample: MemberSummary[]): MemberSummary[] {
  if (!visualParity()) return [...sample, {user_id: "support", display_name: "Support reviewer", email: "support@example.test", role: "member", joined_at: "2026-11-08T10:00:00Z"}];
  return [
    {user_id: "fixture-owner", display_name: "Kishore", email: "kishore@example.com", role: "owner", joined_at: "2026-11-01T10:00:00Z"},
    {user_id: "maya", display_name: "Maya Rao", email: "maya.rao@example.com", role: "member", joined_at: "2026-11-08T10:00:00Z"},
  ];
}

export function visualInvites(sample: InviteSummary[]): InviteSummary[] {
  return visualParity() ? [
    {...sample[0], id: "leon", email: "leon.park@example.com", role: "member", created_at: "2026-11-11T10:00:00Z", expires_at: "2026-11-18T10:00:00Z", link: "https://example.test/leon", email_status: "sent", email_sent_at: "2026-11-11T10:00:00Z"},
    {...sample[0], id: "sofia", email: "sofia.chen@example.com", role: "member", created_at: "2026-11-11T10:00:00Z", expires_at: "2026-11-18T10:00:00Z", link: "https://example.test/sofia", email_status: "failed", email_sent_at: null},
  ] : sample;
}

export function visualCatalogueModel() {
  return visualParity() ? {...CATALOGUE_MODEL, id: "gpt-4.1", display_name: "GPT-4.1", provider: "openai"} : CATALOGUE_MODEL;
}

export function visualModels(sample: TenantModelEntryPageResult): TenantModelEntryPageResult {
  const model = sample.models[0];
  return !visualParity() || !model ? sample : {...sample,
    models: [{...model, model_id: "gpt-4.1", display_name: "GPT-4.1", provider: "openai", secret_ref: "OpenAI workspace", active: false}],
  };
}
