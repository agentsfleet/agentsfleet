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

/// The operator's catalog view (GET /v1/admin/fleet-libraries). Unlike every
/// tenant-facing read, this one filters on NOTHING: a draft, and a row whose
/// bundle was never fetched, are exactly what the operator needs to see — the
/// page exists to answer "what is live, and what still needs work".
///
/// Metadata only. `skill_markdown`, `trigger_markdown`, and the support-file
/// bodies are never selected, so an object-store key cannot leak through this
/// projection (M128 Invariant 3).
pub const SELECT_ADMIN_CATALOG =
    \\SELECT id, name, description, source_repo, source_ref, visibility, content_hash,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text,
    \\       COALESCE(support_files_json::text, '[]'), (trigger_markdown IS NOT NULL),
    \\       updated_at
    \\  FROM core.fleet_library
    \\ ORDER BY id
;

/// Read one row's lifecycle facts. Backs three guards: publish-needs-a-bundle,
/// delete-needs-unpublished, and the add-path's id-collision check (whether this
/// id already belongs to a DIFFERENT repository).
pub const SELECT_CATALOG_ROW =
    \\SELECT source_repo, visibility, content_hash
    \\  FROM core.fleet_library
    \\ WHERE id = $1
;

/// The operator's pencil: the two fields no bundle can supply. `description` and
/// `required_credentials_reasons` are absent from INSERT_PLATFORM's ON CONFLICT
/// list precisely so a refetch cannot undo what this wrote (M128 Invariant 4).
/// COALESCE keeps the statement a partial update — a null argument leaves the
/// column alone, so editing one field never blanks the other.
pub const UPDATE_CATALOG_CURATE =
    \\UPDATE core.fleet_library
    \\   SET description = COALESCE($2, description),
    \\       required_credentials_reasons = COALESCE($3::jsonb, required_credentials_reasons),
    \\       updated_at = $4
    \\ WHERE id = $1
    \\RETURNING id
;

/// Publish / unpublish. The `content_hash IS NOT NULL` arm is the invariant, not
/// a convenience: a published row ALWAYS has a bundle, so a publish of an empty
/// row updates zero rows and the handler turns that into UZ-CATALOG-002. Guarding
/// it here means the database enforces it even if a caller bypasses the handler.
pub const UPDATE_CATALOG_VISIBILITY =
    \\UPDATE core.fleet_library
    \\   SET visibility = $2, updated_at = $3
    \\ WHERE id = $1
    \\   AND ($2 <> $4 OR content_hash IS NOT NULL)
    \\RETURNING id
;

/// Delete an unpublished entry. Scoped to `visibility <> $2` ('public'): a live
/// fleet is never deleted out from under the tenants who can install it —
/// withdraw it first. Zero rows affected on a published row, which the handler
/// turns into UZ-CATALOG-003.
pub const DELETE_CATALOG_DRAFT =
    \\DELETE FROM core.fleet_library
    \\ WHERE id = $1 AND visibility <> $2
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
