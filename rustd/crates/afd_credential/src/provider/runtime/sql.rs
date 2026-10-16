//! Sealed generations are hashed in Postgres; setup review never receives a key.

macro_rules! version {
    ($selection:literal) => { concat!(
        "encode(sha256(convert_to(jsonb_build_array(", $selection,
        ",s.id,s.kek_version,s.meta_provider,s.meta_base_url,encode(sha256(s.ciphertext),'hex'),",
        "r.revision)::text,'UTF8')),'hex')"
    ) };
}

pub(super) const ENTRY: &str = concat!(
    "SELECT s.meta_provider,e.model_id,GREATEST(coalesce(m.cap,0),0),",
    version!("jsonb_build_array(e.id,e.model_id,e.secret_ref,m.cap)"),
    ",s.workspace_id::text,s.key_name,s.meta_base_url
FROM core.tenant_model_entries e
JOIN LATERAL (SELECT id FROM core.workspaces WHERE tenant_id=e.tenant_id ORDER BY created_at,id LIMIT 1) w ON true
JOIN vault.secrets s ON s.workspace_id=w.id AND s.key_name=e.secret_ref
LEFT JOIN LATERAL (SELECT min(context_cap_tokens) AS cap FROM core.model_library
 WHERE model_id=e.model_id AND (provider=s.meta_provider OR s.meta_provider='openai-compatible')) m ON true
CROSS JOIN core.model_catalogue_revision r
WHERE e.tenant_id=$1::uuid AND e.id::text=$2
AND (s.meta_has_key OR s.meta_provider='openai-compatible')
AND (m.cap IS NOT NULL OR s.meta_provider='openai-compatible')
FOR SHARE OF e,s,r"
);

pub(super) const PLATFORM: &str = concat!(
    "SELECT p.provider,p.model,GREATEST(p.context_cap_tokens,0),",
    version!("jsonb_build_array(p.provider,p.model,p.base_url,p.context_cap_tokens)"),
    ",s.workspace_id::text,s.key_name,p.base_url
FROM core.platform_provider_defaults p
JOIN vault.secrets s ON s.workspace_id=p.source_workspace_id AND s.key_name=p.provider
CROSS JOIN core.model_catalogue_revision r
WHERE p.active AND s.meta_provider=p.provider
AND (s.meta_has_key OR p.provider='openai-compatible')
AND p.model IS NOT NULL AND EXISTS(SELECT 1 FROM core.tenants WHERE id=$1::uuid)
AND $2::text IS NOT NULL ORDER BY p.updated_at DESC,p.provider DESC LIMIT 1
FOR SHARE OF p,s,r"
);
