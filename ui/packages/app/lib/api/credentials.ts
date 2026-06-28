import { request } from "./client";
import { CREDENTIAL_FIELD } from "@/lib/types";

// Workspace credential vault. The plaintext body stays opaque — never returned
// on read — but the list now carries a non-secret metadata *projection* the
// server derives by decrypting each body and extracting everything but
// `api_key` (see src/agentsfleetd/http/handlers/fleets/credential_metadata.zig).
// The client reads that projection instead of guessing what each credential is.

// Credential kinds, keyed off the server's `kind` discriminator. The string
// values are verbatim with the Zig `@tagName` in credential_metadata.zig's
// `Kind` enum (RULE UFS — cross-runtime parity); changing one without the other
// silently breaks classification.
export const CREDENTIAL_KIND = {
  provider_key: "provider_key",
  custom_endpoint: "custom_endpoint",
  custom_secret: "custom_secret",
} as const;

export type CredentialKind = (typeof CREDENTIAL_KIND)[keyof typeof CREDENTIAL_KIND];

// Common descriptors every credential carries regardless of kind. Display-only
// surfaces (the secret list, the vault table) read just these.
interface CredentialBase {
  name: string;
  /** Epoch milliseconds — `vault.secrets.created_at`, serialized as int64. */
  created_at: number;
}

/**
 * A vault credential as the list projects it — a tagged union keyed by server
 * `kind`. The server decides what each credential *is* (by the stored `provider`
 * field, never the user-chosen name), so the client classifies by reading `kind`
 * and never re-derives it from a name or provider string (spec Invariant #3).
 * `api_key` is structurally absent: the projection has no field for it.
 *
 * Optional `model`/`base_url` ride the `emit_null_optional_fields=false` wire
 * shape — a credential that never stored them simply omits the key.
 */
export type Credential =
  | ({ kind: typeof CREDENTIAL_KIND.provider_key; provider: string; model?: string } & CredentialBase)
  | ({
      kind: typeof CREDENTIAL_KIND.custom_endpoint;
      provider: string;
      model?: string;
      base_url?: string;
    } & CredentialBase)
  | ({ kind: typeof CREDENTIAL_KIND.custom_secret } & CredentialBase);

export type ProviderKeyCredential = Extract<Credential, { kind: typeof CREDENTIAL_KIND.provider_key }>;
export type CustomEndpointCredential = Extract<
  Credential,
  { kind: typeof CREDENTIAL_KIND.custom_endpoint }
>;
export type CustomSecretCredential = Extract<Credential, { kind: typeof CREDENTIAL_KIND.custom_secret }>;

/**
 * The decrypted credential body the vault stores (never returned on read). The
 * shape is open (`[key: string]: unknown`) because a SKILL.md references
 * arbitrary fields by name, but the self-managed model-provider fields are
 * typed so the own-key + custom-endpoint write paths can't drift: `provider` +
 * optional `base_url` (required iff `provider === OPENAI_COMPATIBLE_PROVIDER`,
 * https + SSRF-validated server-side) alongside `api_key` / `model`.
 */
export interface CredentialData {
  provider?: string;
  api_key?: string;
  model?: string;
  base_url?: string;
  [key: string]: unknown;
}

export interface CredentialListResponse {
  credentials: Credential[];
}

// Classification helpers — the single place the union narrows by kind, so no
// caller re-implements the discriminator check. Each returns the narrowed
// variant type so consumers read `provider`/`model`/`base_url` without a cast.

/** Stored named-provider keys (anthropic, openai, …) — the switch-list rows. */
export function providerKeysOf(credentials: Credential[]): ProviderKeyCredential[] {
  return credentials.filter(
    (c): c is ProviderKeyCredential => c.kind === CREDENTIAL_KIND.provider_key,
  );
}

/** Stored OpenAI-compatible custom endpoints — carry a `base_url`. */
export function customEndpointsOf(credentials: Credential[]): CustomEndpointCredential[] {
  return credentials.filter(
    (c): c is CustomEndpointCredential => c.kind === CREDENTIAL_KIND.custom_endpoint,
  );
}

/** Opaque named secrets a SKILL.md reads by field path — never model providers. */
export function customSecretsOf(credentials: Credential[]): CustomSecretCredential[] {
  return credentials.filter(
    (c): c is CustomSecretCredential => c.kind === CREDENTIAL_KIND.custom_secret,
  );
}

export async function listCredentials(
  workspaceId: string,
  token: string,
): Promise<CredentialListResponse> {
  return request<CredentialListResponse>(
    `/v1/workspaces/${workspaceId}/credentials`,
    { method: "GET" },
    token,
  );
}

export async function createCredential(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
  token: string,
): Promise<{ name: string }> {
  return request<{ name: string }>(
    `/v1/workspaces/${workspaceId}/credentials`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

/**
 * Rotate only the secret of an existing credential — `PATCH …/credentials/{name}`
 * with `{ api_key }`. The server preserves every non-secret field
 * (provider/model/base_url), so this is Replace-key-safe for every kind and
 * cannot corrupt a custom endpoint's `base_url`. A missing name returns a typed
 * 404; an empty/oversized key returns a typed 400.
 */
export async function rotateCredential(
  workspaceId: string,
  name: string,
  apiKey: string,
  token: string,
): Promise<{ name: string }> {
  return request<{ name: string }>(
    `/v1/workspaces/${workspaceId}/credentials/${encodeURIComponent(name)}`,
    { method: "PATCH", body: JSON.stringify({ [CREDENTIAL_FIELD.apiKey]: apiKey }) },
    token,
  );
}

export async function deleteCredential(
  workspaceId: string,
  name: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/credentials/${encodeURIComponent(name)}`,
    { method: "DELETE" },
    token,
  );
}
