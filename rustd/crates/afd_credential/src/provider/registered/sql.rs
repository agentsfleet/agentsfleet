//! Explicit registered-model metadata, scoped to the workspace's billing owner.

macro_rules! prices {
    () => {
        "m.context_cap_tokens, m.input_nanos_per_mtok, m.output_nanos_per_mtok"
    };
}

// Ciphertext changes even when rotation shares a millisecond with its
// predecessor. Hashing its sealed bytes in the database detects that change
// without returning or opening them. JSON array boundaries prevent collisions
// between a model reference and the metadata fields beside it.
macro_rules! version {
    ($reference:literal) => {
        concat!(
            "encode(sha256(convert_to(jsonb_build_array(",
            $reference,
            ", s.id, s.meta_provider, s.meta_base_url, encode(sha256(s.ciphertext), 'hex'),
         m.model_id, m.context_cap_tokens, m.input_nanos_per_mtok,
         m.output_nanos_per_mtok, r.revision)::text, 'UTF8')), 'hex')"
        )
    };
}

/// Every column after the model identity: its prices, then the version digest.
/// One expansion, so the owner and platform selects cannot order them apart.
macro_rules! priced_version {
    ($reference:literal) => {
        concat!(prices!(), ", ", version!($reference))
    };
}

/// `$1` owner, `$2` opaque reference. Comparing the UUID as text rejects an
/// unknown opaque reference as absence without attempting an unsafe UUID cast.
pub(super) const ENTRY: &str = concat!(
    "SELECT s.meta_provider, e.model_id, ",
    priced_version!("e.id"),
    ", s.workspace_id::text, s.key_name
FROM core.tenant_model_entries e
JOIN LATERAL (
    SELECT id FROM core.workspaces WHERE tenant_id = e.tenant_id
    ORDER BY created_at, id LIMIT 1
) w ON true
JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
JOIN core.model_library m ON m.provider = s.meta_provider AND m.model_id = e.model_id
CROSS JOIN core.model_catalogue_revision r
WHERE e.tenant_id = $1::uuid AND e.id::text = $2
  AND s.meta_has_key = true AND s.meta_base_url IS NULL
FOR SHARE OF e,s,m,r"
);

/// The platform choice is explicit; an absent owner entry never selects it.
pub(super) const PLATFORM: &str = concat!(
    "SELECT p.provider, p.model, ",
    priced_version!("p.provider"),
    ", s.workspace_id::text, s.key_name
FROM core.platform_provider_defaults p
JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
JOIN core.model_library m ON m.provider = p.provider AND m.model_id = p.model
CROSS JOIN core.model_catalogue_revision r
WHERE p.active = true AND p.base_url IS NULL AND s.meta_has_key = true
  AND s.meta_provider = p.provider
  AND EXISTS (SELECT 1 FROM core.tenants WHERE id = $1::uuid)
  AND $2::text IS NOT NULL
ORDER BY p.provider LIMIT 1 FOR SHARE OF p,s,m,r"
);
