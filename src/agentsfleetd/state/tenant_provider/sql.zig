//! Centralized SQL for tenant provider selection reads and writes.

pub const UPSERT_SELF_MANAGED =
    \\INSERT INTO core.tenant_model_selection
    \\  (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $7)
    \\ON CONFLICT (tenant_id) DO UPDATE SET
    \\  mode               = EXCLUDED.mode,
    \\  provider           = EXCLUDED.provider,
    \\  model              = EXCLUDED.model,
    \\  context_cap_tokens = EXCLUDED.context_cap_tokens,
    \\  secret_ref         = EXCLUDED.secret_ref,
    \\  updated_at         = EXCLUDED.updated_at
;

pub const UPSERT_PLATFORM =
    \\INSERT INTO core.tenant_model_selection
    \\  (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4, $5, NULL, $6, $6)
    \\ON CONFLICT (tenant_id) DO UPDATE SET
    \\  mode               = EXCLUDED.mode,
    \\  provider           = EXCLUDED.provider,
    \\  model              = EXCLUDED.model,
    \\  context_cap_tokens = EXCLUDED.context_cap_tokens,
    \\  secret_ref         = NULL,
    \\  updated_at         = EXCLUDED.updated_at
;

// mode is bound as a parameter (not inlined) per the no-static-strings-in-SQL rule.
pub const SELECT_ACTIVE_SELF_MANAGED_REF =
    \\SELECT secret_ref, model
    \\FROM core.tenant_model_selection
    \\WHERE tenant_id = $1::uuid AND mode = $2 AND secret_ref IS NOT NULL
;
