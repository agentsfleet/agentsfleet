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
/// `name` and `description` are ABSENT from the ON CONFLICT list on purpose: both
/// are operator-owned after creation (the pencil on /admin/fleet-libraries writes
/// them), and a refetch that clobbered the operator's copy would make that edit a
/// lie (M128 Invariant 4; M130 §3 moved `name` across the same line — offering a
/// rename the next Fetch update silently reverts is worse than not offering it).
/// A brand-new row still takes both from the bundle, via the INSERT arm.
///
/// `required_credentials_reasons` is neither overwritten NOR left alone: it is
/// PRUNED to the credentials the incoming bundle actually declares. The map is
/// keyed by credential name, so a bundle that drops a credential would otherwise
/// leave a dead key that the dialog never renders and every save faithfully
/// round-trips (M130 §4). Kept keys keep their copy; departed ones are dropped. One guard on
/// the prune: a bundle that declares NO credentials at all (e.g. a version
/// shipped without TRIGGER.md — requirements derive from the trigger) PRESERVES
/// the map instead of wiping it, so a transient authoring state cannot destroy
/// curated copy that the very next version would need again.
///
/// The id-collision guard is IN the statement ($15 = replace), not a SELECT before
/// it. The catalog id comes from the bundle's frontmatter, so two operators adding
/// two DIFFERENT repositories whose bundles declare the same unused name would both
/// see "no such row" in a pre-check and both proceed — and the second upsert would
/// silently swap the first repository's bundle out from under it. The `DO UPDATE
/// ... WHERE` makes the conflict path refuse unless the incumbent IS this repository
/// (a refetch) or the operator said `replace`. Zero rows returned means the id
/// belongs to someone else; the caller turns that into UZ-CATALOG-004.
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
    \\   source_repo = EXCLUDED.source_repo,
    \\   source_ref = EXCLUDED.source_ref,
    \\   required_credentials = EXCLUDED.required_credentials,
    \\   required_credentials_reasons = CASE
    \\       WHEN jsonb_array_length(EXCLUDED.required_credentials) = 0
    \\       THEN core.fleet_library.required_credentials_reasons
    \\       ELSE (
    \\           SELECT COALESCE(jsonb_object_agg(k, v), '{}'::jsonb)
    \\             FROM jsonb_each_text(core.fleet_library.required_credentials_reasons) AS r(k, v)
    \\            WHERE r.k IN (
    \\                  SELECT jsonb_array_elements_text(EXCLUDED.required_credentials)
    \\            )
    \\       ) END,
    \\   required_tools = EXCLUDED.required_tools,
    \\   network_hosts = EXCLUDED.network_hosts,
    \\   visibility = EXCLUDED.visibility,
    \\   content_hash = EXCLUDED.content_hash,
    \\   skill_markdown = EXCLUDED.skill_markdown,
    \\   trigger_markdown = EXCLUDED.trigger_markdown,
    \\   support_files_json = EXCLUDED.support_files_json,
    \\   updated_at = EXCLUDED.updated_at
    \\ WHERE $15::boolean OR core.fleet_library.source_repo = EXCLUDED.source_repo
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

/// The operator's view of ONE row. Column-for-column identical to
/// SELECT_ADMIN_CATALOG, so both decode through the same mapper and the indices
/// cannot drift apart.
pub const SELECT_ADMIN_CATALOG_ROW =
    \\SELECT id, name, description, source_repo, source_ref, visibility, content_hash,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text,
    \\       COALESCE(support_files_json::text, '[]'), (trigger_markdown IS NOT NULL),
    \\       updated_at
    \\  FROM core.fleet_library
    \\ WHERE id = $1
;

/// Read one row's lifecycle facts + its editable surface. Backs five guards:
/// publish-needs-a-bundle, delete-needs-unpublished, the add-path's
/// id-collision check (whether this id already belongs to a DIFFERENT
/// repository), the PATCH's publish-vs-source-change check (you cannot publish
/// a bundle you are discarding in the same request), and the PATCH's `If-Match`
/// optimistic-concurrency verdict — which hashes `name` … `visibility` (NOT
/// `content_hash`, so a bundle refetch does not 412 an unrelated description
/// edit). Column order here is the ETag field order in `catalog.rowSurface`.
pub const SELECT_CATALOG_ROW =
    \\SELECT source_repo, visibility, content_hash, source_ref,
    \\       name, description, required_credentials_reasons::text
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

/// The operator's identity edit: name, and the source the bundle came from.
///
/// The invalidation lives HERE, not in the handler, and that is the point. In an
/// UPDATE's SET list the right-hand side reads the OLD row, so
/// `COALESCE($3, source_repo) IS DISTINCT FROM source_repo` asks "is the incoming
/// repository different from the stored one?" atomically with the write — no
/// read-then-write window, and no way for a caller to change the source while
/// keeping a `content_hash` that was built from the previous one. Both columns
/// move together or neither does (M130 Invariant 3).
///
/// A stale bundle is discarded rather than kept: the tar in object storage was
/// built from the OLD repository, so a row that kept it would advertise a source
/// it is not serving. Workspaces already running the fleet are untouched — their
/// install pinned its own content_hash at install time.
///
/// $5 is the draft visibility: a row whose bundle just went away cannot stay
/// public, and UPDATE_CATALOG_VISIBILITY below would refuse to republish it until
/// a bundle is fetched.
/// THE TWO CASE ARMS BELOW ARE ONE INVARIANT, SPELLED TWICE. `content_hash`
/// and `visibility` must move together or not at all (M130 Invariant 3) — SQL
/// offers no clean way to share the predicate, so any edit to one arm's
/// IS DISTINCT FROM pair MUST be mirrored in the other. The integration test
/// `repointing the repository nulls the bundle and withdraws the fleet` pins
/// the pair; this comment is why it exists.
pub const UPDATE_CATALOG_IDENTITY =
    \\UPDATE core.fleet_library
    \\   SET name = COALESCE($2, name),
    \\       source_repo = COALESCE($3, source_repo),
    \\       source_ref = COALESCE($4, source_ref),
    \\       content_hash = CASE
    \\           WHEN COALESCE($3, source_repo) IS DISTINCT FROM source_repo
    \\             OR COALESCE($4, source_ref) IS DISTINCT FROM source_ref
    \\           THEN NULL ELSE content_hash END,
    \\       visibility = CASE
    \\           WHEN COALESCE($3, source_repo) IS DISTINCT FROM source_repo
    \\             OR COALESCE($4, source_ref) IS DISTINCT FROM source_ref
    \\           THEN $5 ELSE visibility END,
    \\       updated_at = $6
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

/// The public bundles list (GET /v1/fleets/bundles). Filters on BOTH the publish
/// state and the bundle's presence — the same pair the gallery and the install path
/// use, so all three reads agree on what a tenant can see.
///
/// The `content_hash IS NOT NULL` arm is not belt-and-braces: without it a row that
/// is `public` but holds no bundle is ADVERTISED here and then dead-ends at install,
/// because SELECT_PLATFORM_INSTALL requires the hash. That is how the pre-M128
/// seed rows behaved. Enforcing it in the query rather than migrating the rows means
/// a stale row cannot lie to a tenant no matter what state it is in.
pub const SELECT_BUNDLES_LIST =
    \\SELECT id, name, description,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text
    \\  FROM core.fleet_library
    \\ WHERE visibility = $1 AND content_hash IS NOT NULL
    \\ ORDER BY id
;
