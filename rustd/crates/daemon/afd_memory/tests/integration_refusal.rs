//! What a refusal answers, by who asked: the runner plane's verbs keep the
//! internal database codes, the operator surface names the operation that
//! failed, and a refusal before any statement is reported unchanged.
//!
//! The last needs no datastore and runs in the unit lane too.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code::{
    INTERNAL_DB_QUERY, INTERNAL_DB_UNAVAILABLE, INTERNAL_OPERATION_FAILED, MEM_UNAVAILABLE,
};
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_uuid7, unreachable_db};
use afd_memory::error::detail::{DATABASE_ERROR, LIST_FAILED, OPERATION_FAILED, ROLE_SWITCH};
use afd_memory::page::View;
use afd_memory::{Memories, Owner, PgStore};
use afd_wire::memory::{PINNED_CATEGORY, Visibility};

use crate::workspace::delta;

/// The instant every write here is stamped with.
const AT: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_runner_statement_the_database_refuses_answers_the_query_code() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let store = PgStore::new(database.clone(), Entropy::new());
    // Neither row exists, so the entry's foreign keys refuse the insert.
    let (workspace, fleet) = (mint_uuid7(), mint_uuid7());
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };
    let orphan = delta("orphan", PINNED_CATEGORY, Visibility::Fleet);

    let refused = store
        .upsert(owner, &[&orphan], AT)
        .await
        .expect_err("an entry for a fleet that does not exist is refused");

    assert_eq!(
        (refused.code(), refused.detail()),
        (INTERNAL_DB_QUERY, DATABASE_ERROR)
    );
    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_operator_statement_the_database_refuses_answers_its_operation_sentence() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let store = PgStore::new(database.clone(), Entropy::new());
    let (workspace, fleet) = (mint_uuid7(), mint_uuid7());
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };

    // Postgres refuses a negative LIMIT outright.
    let refused = store
        .page(owner, false, View::Recent, None, -1)
        .await
        .expect_err("a page under a negative limit is refused");

    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_UNAVAILABLE, LIST_FAILED)
    );
    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_upsert_whose_identifier_cannot_be_minted_answers_operation_failed() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let (entropy, ctrl) = Entropy::new_mocked();
    ctrl.fail_next();
    let store = PgStore::new(database.clone(), entropy);
    let (workspace, fleet) = (mint_uuid7(), mint_uuid7());
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };
    let entry = delta("unminted", PINNED_CATEGORY, Visibility::Fleet);

    let refused = store
        .upsert(owner, &[&entry], AT)
        .await
        .expect_err("an entry with no identifier to write under is refused");

    assert_eq!(
        (refused.code(), refused.detail()),
        (INTERNAL_OPERATION_FAILED, OPERATION_FAILED)
    );
    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
async fn test_a_capture_whose_grants_cannot_be_read_reports_the_datastore_refusal() {
    let memories = Memories::new(unreachable_db(), Entropy::new());
    let pushed = [delta("never_stored", PINNED_CATEGORY, Visibility::Fleet)];

    let refused = memories
        .capture(&mint_uuid7(), &pushed, AT)
        .await
        .expect_err("a capture whose grants cannot be read is refused");

    assert!(refused.is_datastore_unavailable(), "{refused}");
    assert_eq!(refused.code(), INTERNAL_DB_UNAVAILABLE);
}

/// A login role holding no grant at all, created by the test: the lane's API
/// login holds `api_runtime`, which may assume `memory_runtime`, and its owner
/// may assume any role, so only a role of the test's own making meets a
/// refused `SET LOCAL ROLE`.
const ROLELESS: &str = "memory_roleless_probe";

/// The lane's URL with its userinfo swapped for [`ROLELESS`], keeping the
/// query string that carries `sslmode`.
fn roleless_url(lane_url: &str) -> String {
    let (scheme, rest) = lane_url
        .split_once("://")
        .expect("the lane's URL carries a scheme");
    let tail = rest.split_once('@').map_or(rest, |(_, tail)| tail);
    format!("{scheme}://{ROLELESS}:{ROLELESS}@{tail}")
}

/// A store whose pool authenticates as [`ROLELESS`].
///
/// The role is created through the lane's owner pool: creating it is setup,
/// and the API login may not create roles.
async fn roleless_store(lane: &TestDatabase) -> PgStore {
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let mut connection = owner.acquire().await.expect("the lane's owner connects");
    // `IF NOT EXISTS` by exception: the lane is shared and a rerun finds it.
    sqlx::query(
        "DO $$ BEGIN \
           EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', 'memory_roleless_probe', \
             'memory_roleless_probe'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$",
    )
    .execute(&mut *connection)
    .await
    .expect("the lane's owner may create a probe role");
    drop(connection);
    owner.close().await;
    let pool = lane
        .open(
            DbRole::Api,
            &[
                (DbRole::Api.url_knob(), &roleless_url(&lane.url())),
                ("DATABASE_POOL_SIZE", "1"),
                ("DATABASE_MIN_POOL_SIZE", "1"),
            ],
        )
        .await;
    PgStore::new(pool, Entropy::new())
}

/// A refused role switch answers by who asked: the runner plane's verb keeps
/// the internal query code, and the operator's names the switch itself rather
/// than the read it never reached.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_refused_role_switch_answers_by_who_asked() {
    let lane = TestDatabase::shared();
    let store = roleless_store(&lane).await;
    let (workspace, fleet) = (mint_uuid7(), mint_uuid7());
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };

    let runner = store
        .window(owner, false)
        .await
        .expect_err("a login outside memory_runtime cannot assume it");
    assert_eq!(
        (runner.code(), runner.detail()),
        (INTERNAL_DB_QUERY, DATABASE_ERROR)
    );

    let operator = store
        .forget(owner, "never_written")
        .await
        .expect_err("a login outside memory_runtime cannot assume it");
    assert_eq!(
        (operator.code(), operator.detail()),
        (MEM_UNAVAILABLE, ROLE_SWITCH)
    );
    lane.cleanup().await;
}
