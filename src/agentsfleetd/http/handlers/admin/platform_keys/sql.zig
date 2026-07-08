//! Centralized SQL for admin platform provider defaults.

pub const SELECT_WORKSPACE_EXISTS =
    "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 LIMIT 1";

pub const UPSERT_ACTIVE_DEFAULT =
    \\INSERT INTO core.platform_provider_defaults
    \\  (id, provider, source_workspace_id, model, base_url, context_cap_tokens, active, created_at, updated_at)
    \\VALUES ($1, $2, $3, $4, $5, $6, true, $7, $7)
    \\ON CONFLICT (provider) DO UPDATE
    \\SET source_workspace_id = EXCLUDED.source_workspace_id,
    \\    model = EXCLUDED.model,
    \\    base_url = EXCLUDED.base_url,
    \\    context_cap_tokens = EXCLUDED.context_cap_tokens,
    \\    active = true,
    \\    updated_at = EXCLUDED.updated_at
;

pub const DEACTIVATE_OTHER_DEFAULTS =
    \\UPDATE core.platform_provider_defaults
    \\   SET active = false, model = NULL, updated_at = $1
    \\ WHERE active = true AND provider <> $2
;

pub const DEACTIVATE_PROVIDER =
    \\UPDATE core.platform_provider_defaults
    \\   SET active = false, model = NULL, updated_at = $1
    \\ WHERE provider = $2
;

pub const SELECT_KEYS =
    \\SELECT provider, source_workspace_id, active, updated_at
    \\  FROM core.platform_provider_defaults
    \\ ORDER BY provider
;
