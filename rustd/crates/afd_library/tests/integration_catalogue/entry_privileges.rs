//! Slot 917's grant, proven by running the removal as the role a request runs as.
//!
//! # Why this does not ask `has_table_privilege`
//!
//! Because the answer and the outcome can disagree, and only one of them is
//! what a request meets. `integration_purge_privileges.rs` is the file that
//! learned it: the suites there connect as the database owner, which bypasses
//! grants entirely, so a `DELETE` the runtime role had never been entitled to
//! run stayed green for fifteen days. A catalogue view of a privilege is a
//! second source of truth, and this lane already has the first one.
//!
//! So the removal runs through a pool authenticating as a login role holding
//! `api_runtime` and nothing else — the daemon's own shape. A slot that never
//! reached the database, or a grant naming the wrong table, fails here.
//!
//! The refusing direction is asserted too. Without it, a later
//! `GRANT ALL … TO api_runtime` would make the test above pass for the wrong
//! reason and take the boundary with it.

use afd_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::Libraries;

/// A login role holding `api_runtime` and nothing else, created by the test.
///
/// `api_runtime` is `NOLOGIN` and cannot be connected as, and `remove_entry`
/// acquires its own connection from the pool rather than taking one — so a
/// `SET ROLE` on a connection of the test's own would not reach the statement
/// under test. A role of our own making is what closes that.
const PROBE: &str = "library_entry_probe";

/// The instant the fixture row is stamped with — small, and far from any real
/// clock, which keeps it at the old end of a lane other suites share.
const SEEDED_AT: i64 = 5_000;

/// Dimension 1.2 — the grant slot 460 withheld, exercised by the real code path.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_api_runtime_holds_delete_on_tenant_fleet_library() {
    let lane = TestDatabase::shared();
    let owner = lane.open(DbRole::Api, &[]).await;

    let workspace = mint_id();
    let entry = mint_id();
    seed_entry(&owner, &workspace, &entry).await;
    grant_probe(&owner).await;

    let workspace_id = Uuid7::parse(&workspace).expect("the seeded workspace id parses");
    let entry_id = Uuid7::parse(&entry).expect("the seeded entry id parses");

    let restricted = Libraries::new(probe_pool(&lane).await);
    assert!(
        restricted
            .remove_entry(&workspace_id, &entry_id)
            .await
            .expect("api_runtime must hold DELETE on core.tenant_fleet_library"),
        "the seeded row must be the one removed"
    );
    assert!(
        !restricted
            .remove_entry(&workspace_id, &entry_id)
            .await
            .expect("a second removal is not an error"),
        "the row is gone, so the replay removes nothing"
    );
    assert!(
        restricted
            .owned_entries(&workspace_id, 50, None)
            .await
            .expect("the owned collection reads as api_runtime")
            .items
            .is_empty(),
        "the removed entry must leave the owned collection"
    );

    assert_truncate_is_refused(&lane).await;

    drop(owner);
    lane.cleanup().await;
}

/// The fence, in the refusing direction.
///
/// Slot 917 grants `DELETE` and nothing wider. `TRUNCATE` is the neighbouring
/// privilege a careless `GRANT ALL` would hand over with it, and it empties the
/// table for every workspace at once rather than removing one row.
async fn assert_truncate_is_refused(lane: &TestDatabase) {
    let restricted = probe_pool(lane).await;
    let mut connection = restricted.acquire().await.expect("the probe pool connects");
    let refused = sqlx::query("TRUNCATE core.tenant_fleet_library")
        .execute(&mut *connection)
        .await;
    assert!(
        refused.is_err(),
        "api_runtime must not be able to empty the tenant library"
    );
}

/// A tenant, a workspace under it, and one library entry in that workspace.
///
/// `tenant_fleet_library.workspace_id` is a real foreign key into
/// `core.workspaces`, so a minted identifier alone is refused — and rightly.
async fn seed_entry(database: &afd_db::Db, workspace: &str, entry: &str) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, 'Entry privilege fixture', 1, 1) \
           RETURNING id \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         SELECT $2::uuid, id, $2, 'test', 1 FROM tenant",
    )
    .bind(mint_id())
    .bind(workspace)
    .execute(&mut *connection)
    .await
    .expect("the privilege fixture's scope seeds");

    sqlx::query(
        "INSERT INTO core.tenant_fleet_library ( \
           id, workspace_id, name, description, source_kind, source_ref, visibility, \
           content_hash, skill_markdown, trigger_markdown, support_files_json, \
           requirements_json, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, 'privilege-fixture', 'tenant fixture', 'github', 'main', \
           'tenant', $3, '# Fixture', NULL, '[]', \
           '{\"credentials\":[],\"tools\":[],\"network_hosts\":[],\"trigger_present\":false}', \
           $4, $4)",
    )
    .bind(entry)
    .bind(workspace)
    .bind(format!("hash-{entry}"))
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await
    .expect("the privilege fixture's entry seeds");
}

/// Creates the probe role through the owner connection — setup, not the subject.
///
/// `IF NOT EXISTS` in spirit, via the duplicate arm: the lane's database is
/// shared and two suites can reach this at once.
async fn grant_probe(database: &afd_db::Db) {
    let mut connection = database.acquire().await.expect("an API connection");
    for statement in [
        "DO $$ BEGIN \
           EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', 'library_entry_probe', 'library_entry_probe'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$",
        "GRANT api_runtime TO library_entry_probe",
    ] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the lane's owner may create and grant to a probe role");
    }
}

/// The lane's URL with the probe's credentials swapped in.
///
/// Rebuilt from the lane's own URL rather than assembled, so query parameters
/// it carries — `sslmode` among them — survive into the probe's connection.
fn probe_url(lane_url: &str) -> String {
    let (scheme, rest) = lane_url
        .split_once("://")
        .expect("the lane's URL carries a scheme");
    let tail = rest.split_once('@').map_or(rest, |(_, tail)| tail);
    format!("{scheme}://{PROBE}:{PROBE}@{tail}")
}

/// A pool authenticating as [`PROBE`], pinned to one connection.
///
/// One connection is all these statements need, and a second full-size pool
/// warming beside the lane's fault suites is enough connection pressure to make
/// their millisecond budgets miss — a failure in their file with its cause in
/// this one.
async fn probe_pool(lane: &TestDatabase) -> afd_db::Db {
    lane.open(
        DbRole::Api,
        &[
            (DbRole::Api.url_knob(), &probe_url(&lane.url())),
            ("DATABASE_POOL_SIZE", "1"),
            ("DATABASE_MIN_POOL_SIZE", "1"),
        ],
    )
    .await
}
