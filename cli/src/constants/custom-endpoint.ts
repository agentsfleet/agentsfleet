/**
 * Provider-classification constants for the CLI — the two provider kinds whose
 * credential rules differ from a hosted vendor's: the custom OpenAI-compatible
 * endpoint, and the local runtimes.
 *
 * The `provider` id below mirrors the backend resolver
 * (`src/agentsfleetd/state/tenant_provider_resolver.zig` →
 * `OPENAI_COMPATIBLE_PROVIDER`): a self-managed secret whose JSON carries
 * this provider opts into a custom endpoint, where `base_url` is required (and
 * forbidden for every named provider). The CLI re-states the literal once here
 * — the `cli/` and `ui/` Bun projects do not share a module graph, so each side
 * mirrors the Zig source in exactly one place (the same pattern `PROVIDER_MODE`
 * already follows in `constants/billing.ts`). Every reader — the secret-create
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

/**
 * Providers that serve models from hardware the operator owns, mirroring
 * `LOCAL_RUNTIME_PROVIDERS` in `src/agentsfleetd/secrets/metadata.zig` (same
 * identifier, per the cross-runtime naming rule). The server exempts these from
 * two checks — catalogue membership and the non-empty `api_key` — because the
 * served model is whatever the operator loaded and the server authenticates
 * nobody. The CLI has to know the same set or it rejects the credential before
 * the request is ever made, which is what it did.
 *
 * `scripts/check_model_allowlist.py` requires this list, the Zig list, and the
 * allowlist's `activation_floor` set to be equal, so the mirror cannot drift.
 */
export const LOCAL_RUNTIME_PROVIDERS = [
  "litellm",
  "llama.cpp",
  "llamacpp",
  "lm-studio",
  "lmstudio",
  "ollama",
  "osaurus",
  "sglang",
  "vllm",
] as const;

/** Whether this provider serves models from the operator's own hardware. */
export const isLocalRuntime = (provider: string): boolean =>
  (LOCAL_RUNTIME_PROVIDERS as ReadonlyArray<string>).includes(provider);

// Secret JSON field names (verbatim with the server-side resolver).
export const SECRET_FIELD_PROVIDER = "provider" as const;
export const SECRET_FIELD_API_KEY = "api_key" as const;
export const SECRET_FIELD_BASE_URL = "base_url" as const;
export const SECRET_FIELD_MODEL = "model" as const;

// The only scheme a custom endpoint may use — checked client-side so a typo'd
// `http://` URL never reaches the network (the server-side guard re-checks and
// also rejects SSRF-unsafe hosts).
export const HTTPS_SCHEME_PREFIX = "https://" as const;
