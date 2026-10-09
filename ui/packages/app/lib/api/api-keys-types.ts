// The tenant API key bounds and sort order the settings surfaces share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export const API_KEY_SORTS = ["-created_at", "created_at", "-key_name", "key_name"] as const;

export type ApiKeySort = (typeof API_KEY_SORTS)[number];

export const DEFAULT_SORT: ApiKeySort = "-created_at";

// Mirrors the daemon's constants verbatim: `TENANT_API_KEY_PREFIX` in
// rustd/crates/afd_auth/src/credential.rs; `KeyName` (1-64 chars, alnum + - + _)
// and `DESCRIPTION_MAX` in rustd/crates/afd_tenant/src/apikey/name.rs; and the
// sort allowlist above, `ApiKeySort` in rustd/crates/afd_tenant/src/apikey/sort.rs.
export const KEY_PREFIX = "agt_t";
export const KEY_NAME_REGEX = /^[A-Za-z0-9_-]{1,64}$/;
export const KEY_NAME_MAX = 64;
export const DESCRIPTION_MAX = 256;
