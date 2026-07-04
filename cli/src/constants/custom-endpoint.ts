/**
 * Custom OpenAI-compatible endpoint constants for the CLI.
 *
 * The `provider` id below mirrors the backend resolver
 * (`src/agentsfleetd/state/tenant_provider_resolver.zig` →
 * `OPENAI_COMPATIBLE_PROVIDER`): a self-managed secret whose JSON carries
 * this provider opts into a custom endpoint, where `base_url` is required (and
 * forbidden for every named provider). The CLI re-states the literal once here
 * — the `cli/` and `ui/` Bun projects do not share a module graph, so each side
 * mirrors the Zig source in exactly one place (the same pattern `PROVIDER_MODE`
 * already follows in `constants/billing.ts`). Every reader — the secret-add
 * command, the option validator, and the tests — imports from here (RULE UFS).
 *
 * The secret JSON field keys match the resolver's `S_API_KEY` / `S_BASE_URL`
 * extraction (`api_key` / `base_url`); the `custom`-flow flags compose a
 * `{ provider, api_key, base_url, model? }` object posted to the vault, so a
 * non-https `base_url` is rejected by a commander option validator (exit 2, no
 * network call) while full SSRF validation stays server-side in
 * `base_url_guard.zig`.
 */

export const OPENAI_COMPATIBLE_PROVIDER = "openai-compatible" as const;

// Secret JSON field names (verbatim with the server-side resolver).
export const SECRET_FIELD_PROVIDER = "provider" as const;
export const SECRET_FIELD_API_KEY = "api_key" as const;
export const SECRET_FIELD_BASE_URL = "base_url" as const;
export const SECRET_FIELD_MODEL = "model" as const;

// The only scheme a custom endpoint may use — checked client-side so a typo'd
// `http://` URL never reaches the network (the server-side guard re-checks and
// also rejects SSRF-unsafe hosts).
export const HTTPS_SCHEME_PREFIX = "https://" as const;
