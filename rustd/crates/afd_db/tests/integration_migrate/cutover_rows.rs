//! Rows written at slot 949 into the bundle storage 950 drops and the native
//! setup that replaces it, and the readings the cutover proofs compare.

use super::*;
use afd_db::Db;
use afd_db::test_util::mint_id;

/// The slot that drops the Markdown bundle storage.
pub(super) const CUTOVER: i32 = 950;

/// One tenant whose fleet came from a Markdown bundle and now has native setup.
///
/// One statement: data-modifying CTEs share a snapshot, and every foreign key
/// is checked when the statement ends, so the parents need not commit first.
const SEED: &str = "\
WITH tenant AS (
  INSERT INTO core.tenants (id, name, created_at, updated_at)
  VALUES ($1::uuid, 'cutover', 1, 2)
), workspace AS (
  INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
  VALUES ($2::uuid, $1::uuid, 'cutover', 'user_cutover', 1)
), platform_bundle AS (
  INSERT INTO core.fleet_library (id, name, description, source_repo, source_path, source_ref,
    required_credentials, required_credentials_reasons, required_tools, network_hosts,
    visibility, content_hash, skill_markdown, trigger_markdown, support_files_json,
    created_at, updated_at)
  VALUES ($4, 'Platform bundle', 'retired', 'agentsfleet/x', '', 'main', '[]', '{}', '[]', '[]',
    'public', 'sha256:platform', '# skill', '# trigger', '[]', 1, 2)
), tenant_bundle AS (
  INSERT INTO core.tenant_fleet_library (id, workspace_id, name, description, source_kind,
    source_ref, visibility, content_hash, skill_markdown, trigger_markdown, support_files_json,
    requirements_json, created_at, updated_at)
  VALUES ($7::uuid, $2::uuid, 'Tenant bundle', 'retired', 'upload', 'r', 'private',
    'sha256:tenant', '# skill', NULL, '[]', '{}', 1, 2)
), library AS (
  INSERT INTO core.fleet_libraries (id, tenant_id, visibility, name, description,
    head_revision_id, resource_version, created_at, updated_at, credential_reasons)
  VALUES ($4::uuid, $1::uuid, 'private', 'Native entry', 'kept', $6::uuid, 3, 1, 2,
    '{\"github\":\"reviews\"}')
), library_revision AS (
  INSERT INTO core.fleet_library_revisions (id, library_id, revision, definition,
    canonical_digest, policy_semantics, created_at)
  VALUES ($6::uuid, $4::uuid, 1, '{\"name\":\"kept\"}', 'v1:digest', 'fleet-setup-v1', 1)
), fleet AS (
  INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, trigger_markdown,
    config_json, status, required_tags, bundle_content_hash, bundle_snapshot_key, created_at,
    updated_at, current_definition_revision_id, setup_version, resource_version)
  VALUES ($3::uuid, $2::uuid, $1::uuid, 'cutover', '# skill', '# trigger', '{\"budget\":1}',
    'active', '{gpu}', 'sha256:tenant', 'fleet-bundles/sha256/t.tar', 1, 2, $5::uuid, 7, 4)
)
INSERT INTO core.fleet_definition_revisions (id, fleet_id, revision, definition,
  canonical_digest, policy_semantics, library_revision_id, created_at, runtime_model_ref)
VALUES ($5::uuid, $3::uuid, 7, '{\"name\":\"kept\"}', 'v1:digest', 'fleet-setup-v1', $6::uuid,
  1, 'model-ref')";

/// The rows the cutover must carry across, whole, as JSON text.
///
/// `core.fleets` is read without the four dropped columns, so the comparison
/// is every surviving column of every surviving row.
const SURVIVORS: &str = "\
WITH kept(rows) AS (SELECT jsonb_build_array(
  (SELECT to_jsonb(t) FROM core.tenants t WHERE t.id = $1::uuid),
  (SELECT to_jsonb(w) FROM core.workspaces w WHERE w.id = $2::uuid),
  (SELECT to_jsonb(f) - ARRAY['source_markdown', 'trigger_markdown',
       'bundle_content_hash', 'bundle_snapshot_key']
     FROM core.fleets f WHERE f.id = $3::uuid),
  (SELECT to_jsonb(d) FROM core.fleet_definition_revisions d WHERE d.fleet_id = $3::uuid),
  (SELECT to_jsonb(l) FROM core.fleet_libraries l WHERE l.id = $4::uuid),
  (SELECT to_jsonb(r) FROM core.fleet_library_revisions r WHERE r.library_id = $4::uuid)))
SELECT rows::text, NOT EXISTS (SELECT 1 FROM jsonb_array_elements(rows) e WHERE e = 'null'::jsonb)
FROM kept";

/// The bundle storage itself, which a failed release must leave intact.
const BUNDLE_ROWS: &str = "\
SELECT jsonb_build_array(
  (SELECT to_jsonb(l) FROM core.fleet_library l WHERE l.id = $1),
  (SELECT jsonb_agg(to_jsonb(t)) FROM core.tenant_fleet_library t WHERE t.workspace_id = $2::uuid),
  (SELECT jsonb_build_array(source_markdown, trigger_markdown, bundle_content_hash,
     bundle_snapshot_key) FROM core.fleets WHERE id = $3::uuid))::text";

/// The fleet's current setup still resolves through its native pointers.
const SETUP_CHAIN: &str = "\
SELECT count(*) FROM core.fleets f
JOIN core.fleet_definition_revisions d ON d.id = f.current_definition_revision_id
JOIN core.fleet_library_revisions r ON r.id = d.library_revision_id
JOIN core.fleet_libraries l ON l.id = r.library_id AND l.head_revision_id = r.id
WHERE f.id = $1::uuid";

pub(super) struct Populated {
    tenant: String,
    workspace: String,
    fleet: String,
    library: String,
}

pub(super) fn before_cutover() -> &'static [Migration] {
    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == CUTOVER)
        .expect("slot 950 is registered");
    MIGRATIONS
        .get(..boundary)
        .expect("the slot's index is in range")
}

/// The canonical list up to and including the cutover, so a later slot's own
/// proof is not folded into this one's ledger assertions.
pub(super) fn through_cutover() -> &'static [Migration] {
    through(CUTOVER)
}

/// The canonical list up to and including `slot`.
pub(super) fn through(slot: i32) -> &'static [Migration] {
    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == slot)
        .expect("the slot is registered");
    MIGRATIONS
        .get(..=boundary)
        .expect("the slot's index is in range")
}

pub(super) fn versions_before_cutover() -> Vec<i32> {
    before_cutover().iter().map(Migration::version).collect()
}

/// Migrate a scratch database to slot 949 and write the populated rows.
pub(super) async fn at_slot_before_cutover(db: &Db) -> Populated {
    let outcome = Migrator::new()
        .with_migrations(before_cutover())
        .run(db)
        .await
        .expect("the list before slot 950 applies");
    assert_eq!(outcome.applied, versions_before_cutover());
    let populated = Populated {
        tenant: mint_id(),
        workspace: mint_id(),
        fleet: mint_id(),
        library: mint_id(),
    };
    sqlx::query(SEED)
        .bind(&populated.tenant)
        .bind(&populated.workspace)
        .bind(&populated.fleet)
        .bind(&populated.library)
        .bind(mint_id())
        .bind(mint_id())
        .bind(mint_id())
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .expect("the populated rows apply at slot 949");
    populated
}

pub(super) async fn survivors(db: &Db, p: &Populated) -> String {
    let (rows, complete): (String, bool) = sqlx::query_as(SURVIVORS)
        .bind(&p.tenant)
        .bind(&p.workspace)
        .bind(&p.fleet)
        .bind(&p.library)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap();
    assert!(complete, "every seeded row is present: {rows}");
    rows
}

pub(super) async fn bundle_rows(db: &Db, p: &Populated) -> String {
    sqlx::query_scalar(BUNDLE_ROWS)
        .bind(&p.library)
        .bind(&p.workspace)
        .bind(&p.fleet)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

pub(super) async fn setup_chain(db: &Db, p: &Populated) -> i64 {
    sqlx::query_scalar(SETUP_CHAIN)
        .bind(&p.fleet)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}
