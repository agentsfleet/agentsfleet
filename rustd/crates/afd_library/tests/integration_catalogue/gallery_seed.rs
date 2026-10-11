//! The rows a workspace gallery reads: a scope, platform entries, tenant entries.
//!
//! Shared by the merge-and-seek proof and the tie-order proof, which seed the
//! same two tables and differ only in what they assert.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::test_util::mint_id;

/// The publication state a tenant can install from.
pub(super) const PUBLISHED: &str = "public";

/// The publication state an operator stages a row in.
pub(super) const DRAFT: &str = "draft";

/// The snapshot a published fixture row points at.
const FIXTURE_HASH: &str = "abc123";

/// A tenant and one workspace under it, which the library rows reference.
///
/// `tenant_fleet_library.workspace_id` is a real foreign key into
/// `core.workspaces` — a minted id alone is refused, and rightly: a library
/// entry belonging to no workspace is exactly the orphan the constraint is
/// there to prevent.
pub(super) async fn seed_scope(database: &afd_db::Db, workspace: &str) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, 'Gallery fixture', 1, 1) \
           RETURNING id \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         SELECT $2::uuid, id, $2, 'test', 1 FROM tenant",
    )
    .bind(mint_id())
    .bind(workspace)
    .execute(&mut *connection)
    .await
    .expect("the gallery scope seeds");
}

/// A published platform row with a snapshot, visible to every workspace.
pub(super) async fn seed_platform(database: &afd_db::Db, id: &str, created_at: i64) {
    seed_platform_as(database, id, created_at, PUBLISHED, Some(FIXTURE_HASH)).await;
}

/// A platform row in a chosen publication state, with or without a snapshot.
///
/// The gallery's platform arm filters on BOTH, so a row that fails either half
/// is one a tenant must never see.
pub(super) async fn seed_platform_as(
    database: &afd_db::Db,
    id: &str,
    created_at: i64,
    visibility: &str,
    content_hash: Option<&str>,
) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.fleet_library ( \
           id, name, description, source_repo, source_path, source_ref, \
           required_credentials, required_credentials_reasons, required_tools, \
           network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
           support_files_json, created_at, updated_at) \
         VALUES ($1, $1, 'platform fixture', $1, '', 'main', \
           '[]', '{}', '[]', '[]', $3, $4, '# Fixture', NULL, '[]', $2, $2)",
    )
    .bind(id)
    .bind(created_at)
    .bind(visibility)
    .bind(content_hash)
    .execute(&mut *connection)
    .await
    .expect("the platform row seeds");
}

/// A row in one workspace's own library, answering its stored id.
pub(super) async fn seed_workspace(
    database: &afd_db::Db,
    workspace: &str,
    name: &str,
    created_at: i64,
) -> String {
    let id = mint_id();
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.tenant_fleet_library ( \
           id, workspace_id, name, description, source_kind, source_ref, visibility, \
           content_hash, skill_markdown, trigger_markdown, support_files_json, \
           requirements_json, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3, 'tenant fixture', 'github', 'main', 'workspace', \
           $5, '# Fixture', NULL, '[]', \
           '{\"credentials\":[],\"tools\":[],\"network_hosts\":[],\"trigger_present\":false}', \
           $4, $4)",
    )
    .bind(&id)
    .bind(workspace)
    .bind(name)
    .bind(created_at)
    // The domain key is (workspace_id, content_hash), so two rows in one
    // workspace need two hashes — a shared literal would make the second seed
    // an upsert over the first and leave the merge with nothing to interleave.
    .bind(format!("hash-{name}"))
    .execute(&mut *connection)
    .await
    .expect("the workspace row seeds");
    id
}

/// The fixture identifier as the store takes it.
pub(super) fn parse(id: &str) -> afd_core::id::Uuid7 {
    afd_core::id::Uuid7::parse(id).expect("the minted fixture id is UUIDv7")
}
