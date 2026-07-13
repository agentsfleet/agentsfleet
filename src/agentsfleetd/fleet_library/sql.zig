//! Centralised SQL for the Fleet library domain (M103). Every query against
//! `core.fleet_library` / `core.tenant_fleet_library` lives here so table names
//! are grep-able from one file and queries are testable in isolation.

/// Resolve-by-id for the install flow. Filters on `visibility` ($2 = 'public'),
/// not just on the bundle's presence: an unpublished fleet must be UNREACHABLE,
/// not merely unlisted (M128 Invariant 2). Without this arm, anyone who knew a
/// draft's id could install it, and Unpublish would be decoration.
pub const SELECT_PLATFORM_INSTALL =
    \\SELECT skill_markdown, trigger_markdown, content_hash
    \\  FROM core.fleet_library
    \\ WHERE id = $1 AND visibility = $2
    \\   AND content_hash IS NOT NULL AND skill_markdown IS NOT NULL
;

pub const SELECT_TENANT_INSTALL =
    \\SELECT skill_markdown, trigger_markdown, content_hash
    \\  FROM core.tenant_fleet_library
    \\ WHERE id = $1::uuid AND workspace_id = $2::uuid
;

/// Add-or-refetch a platform catalog entry. $9 is always VISIBILITY_DRAFT — the
/// caller cannot pass 'public' — so EVERY write stages to draft and publishing
/// stays an explicit, separate act (M128 §1). That is why `visibility` is in the
/// ON CONFLICT list: refetching a newer bundle for a PUBLISHED fleet withdraws it
/// back to draft rather than shipping unreviewed content to every tenant. Safe to
/// withdraw, because an install snapshots the bundle onto
/// `core.fleets.bundle_content_hash` — a workspace already running the fleet is
/// untouched; only NEW installs pause until it is republished.
///
/// `description` and `required_credentials_reasons` are ABSENT from the ON
/// CONFLICT list on purpose: both are operator-owned after creation (the pencil
/// on /admin/fleet-libraries writes them), and a refetch that clobbered the
/// operator's copy would make that edit a lie (M128 Invariant 4). A brand-new row
/// still takes its description from the bundle, via the INSERT arm.
pub const INSERT_PLATFORM =
    \\INSERT INTO core.fleet_library
    \\  (id, name, description, source_repo, source_path, source_ref,
    \\   required_credentials, required_credentials_reasons, required_tools, network_hosts,
    \\   visibility, content_hash, skill_markdown, trigger_markdown, support_files_json,
    \\   created_at, updated_at)
    \\VALUES ($1, $2, $3, $4, $5, $6,
    \\        ($7::jsonb -> 'credentials'), $8::jsonb, ($7::jsonb -> 'tools'), ($7::jsonb -> 'network_hosts'),
    \\        $9, $10, $11, $12, $13::jsonb, $14, $14)
    \\ON CONFLICT (id) DO UPDATE SET
    \\   name = EXCLUDED.name,
    \\   source_repo = EXCLUDED.source_repo,
    \\   source_ref = EXCLUDED.source_ref,
    \\   required_credentials = EXCLUDED.required_credentials,
    \\   required_tools = EXCLUDED.required_tools,
    \\   network_hosts = EXCLUDED.network_hosts,
    \\   visibility = EXCLUDED.visibility,
    \\   content_hash = EXCLUDED.content_hash,
    \\   skill_markdown = EXCLUDED.skill_markdown,
    \\   trigger_markdown = EXCLUDED.trigger_markdown,
    \\   support_files_json = EXCLUDED.support_files_json,
    \\   updated_at = EXCLUDED.updated_at
    \\RETURNING id
;

pub const INSERT_TENANT =
    \\WITH inserted AS (
    \\  INSERT INTO core.tenant_fleet_library
    \\    (id, workspace_id, name, description, source_kind, source_ref, visibility,
    \\     content_hash, skill_markdown, trigger_markdown, support_files_json,
    \\     requirements_json, created_at, updated_at)
    \\  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12::jsonb, $13, $13)
    \\  ON CONFLICT (workspace_id, content_hash) DO NOTHING
    \\  RETURNING id::text
    \\)
    \\SELECT id FROM inserted
    \\UNION ALL
    \\SELECT id::text FROM core.tenant_fleet_library
    \\WHERE workspace_id = $2::uuid AND content_hash = $8
    \\LIMIT 1
;

pub const SELECT_GALLERY_PLATFORM =
    \\SELECT id, name, description, source_repo,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text,
    \\       COALESCE(support_files_json::text, '[]'), (trigger_markdown IS NOT NULL)
    \\  FROM core.fleet_library
    \\ WHERE visibility = $1 AND content_hash IS NOT NULL
    \\ ORDER BY id
;

pub const SELECT_GALLERY_TENANT =
    \\SELECT id::text, name, description, source_ref,
    \\       requirements_json::text, support_files_json::text
    \\  FROM core.tenant_fleet_library
    \\ WHERE workspace_id = $1::uuid
    \\ ORDER BY created_at DESC
;

pub const SELECT_BUNDLES_LIST =
    \\SELECT id, name, description,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text
    \\  FROM core.fleet_library
    \\ WHERE visibility = $1
    \\ ORDER BY id
;
