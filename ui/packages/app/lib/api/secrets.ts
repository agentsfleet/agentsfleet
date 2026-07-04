import { request } from "./client";
import { SECRET_FIELD } from "@/lib/types";

// Workspace credential vault. The plaintext body stays opaque — never returned
// on read — but the list now carries a non-secret metadata *projection* the
// server derives by decrypting each body and extracting everything but
// `api_key` (see src/agentsfleetd/http/handlers/fleets/credential_metadata.zig).
// The client reads that projection instead of guessing what each credential is.

// Secret kinds, keyed off the server's `kind` discriminator. The string
// values are verbatim with the Zig `@tagName` in credential_metadata.zig's
// `Kind` enum (RULE UFS — cross-runtime parity); changing one without the other
// silently breaks classification.
export const SECRET_KIND = {
  provider_key: "provider_key",
  custom_endpoint: "custom_endpoint",
  custom_secret: "custom_secret",
} as const;

export type SecretKind = (typeof SECRET_KIND)[keyof typeof SECRET_KIND];

// Common descriptors every credential carries regardless of kind. Display-only
// surfaces (the secret list, the vault table) read just these.
interface SecretBase {
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
export type Secret =
  | ({ kind: typeof SECRET_KIND.provider_key; provider: string; model?: string } & SecretBase)
  | ({
      kind: typeof SECRET_KIND.custom_endpoint;
      provider: string;
      model?: string;
      base_url?: string;
    } & SecretBase)
  | ({ kind: typeof SECRET_KIND.custom_secret } & SecretBase);

export type ProviderKeySecret = Extract<Secret, { kind: typeof SECRET_KIND.provider_key }>;
export type CustomEndpointSecret = Extract<
  Secret,
  { kind: typeof SECRET_KIND.custom_endpoint }
>;
export type CustomSecret = Extract<Secret, { kind: typeof SECRET_KIND.custom_secret }>;

/**
 * The decrypted credential body the vault stores (never returned on read). The
 * shape is open (`[key: string]: unknown`) because a SKILL.md references
 * arbitrary fields by name, but the self-managed model-provider fields are
 * typed so the own-key + custom-endpoint write paths can't drift: `provider` +
 * optional `base_url` (required iff `provider === OPENAI_COMPATIBLE_PROVIDER`,
 * https + SSRF-validated server-side) alongside `api_key` / `model`.
 */
export interface SecretData {
  provider?: string;
  api_key?: string;
  model?: string;
  base_url?: string;
  [key: string]: unknown;
}

export interface SecretListResponse {
  secrets: Secret[];
}

// Classification helpers — the single place the union narrows by kind, so no
// caller re-implements the discriminator check. Each returns the narrowed
// variant type so consumers read `provider`/`model`/`base_url` without a cast.

/** Stored named-provider keys (anthropic, openai, …) — the switch-list rows. */
export function providerKeysOf(credentials: Secret[]): ProviderKeySecret[] {
  return credentials.filter(
    (c): c is ProviderKeySecret => c.kind === SECRET_KIND.provider_key,
  );
}

/** Stored OpenAI-compatible custom endpoints — carry a `base_url`. */
export function customEndpointsOf(credentials: Secret[]): CustomEndpointSecret[] {
  return credentials.filter(
    (c): c is CustomEndpointSecret => c.kind === SECRET_KIND.custom_endpoint,
  );
}

/** Opaque named secrets a SKILL.md reads by field path — never model providers. */
export function customSecretsOf(credentials: Secret[]): CustomSecret[] {
  return credentials.filter(
    (c): c is CustomSecret => c.kind === SECRET_KIND.custom_secret,
  );
}

export async function listSecrets(
  workspaceId: string,
  token: string,
): Promise<SecretListResponse> {
  return request<SecretListResponse>(
    `/v1/workspaces/${workspaceId}/secrets`,
    { method: "GET" },
    token,
  );
}

export async function createSecret(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
  token: string,
): Promise<{ name: string }> {
  return request<{ name: string }>(
    `/v1/workspaces/${workspaceId}/secrets`,
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
export async function rotateSecret(
  workspaceId: string,
  name: string,
  apiKey: string,
  token: string,
): Promise<{ name: string }> {
  return request<{ name: string }>(
    `/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(name)}`,
    { method: "PATCH", body: JSON.stringify({ [SECRET_FIELD.apiKey]: apiKey }) },
    token,
  );
}

export async function deleteSecret(
  workspaceId: string,
  name: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(name)}`,
    { method: "DELETE" },
    token,
  );
}
