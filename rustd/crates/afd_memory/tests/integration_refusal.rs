//! What a refusal answers, by who asked: the runner plane's verbs keep the
//! internal database codes, the operator surface names the operation that
//! failed, and a refusal before any statement is reported unchanged.
//!
//! The last two need no datastore and run in the unit lane too.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code::{
    INTERNAL_DB_QUERY, INTERNAL_DB_UNAVAILABLE, INTERNAL_OPERATION_FAILED, MEM_UNAVAILABLE,
};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id, unreachable_db};
use afd_memory::error::detail::{DATABASE_ERROR, LIST_FAILED, OPERATION_FAILED};
use afd_memory::page::View;
use afd_memory::{InMemory, Memories, MemoryStore, Owner, PgStore, Record};
use afd_wire::memory::{PINNED_CATEGORY, Visibility};

use crate::workspace::delta;

/// The instant every write here is stamped with.
const AT: i64 = 1_760_000_000_000;

fn minted() -> Uuid7 {
    Uuid7::parse(&mint_id()).expect("a minted id is canonical")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_runner_statement_the_database_refuses_answers_the_query_code() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let store = PgStore::new(database.clone(), Entropy::new());
    // Neither row exists, so the entry's foreign keys refuse the insert.
    let (workspace, fleet) = (minted(), minted());
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };
    let orphan = delta("orphan", PINNED_CATEGORY, Visibility::Fleet);

    let refused = store
        .upsert(owner, &[&orphan], UnixMillis::from_millis(AT))
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
    let (workspace, fleet) = (minted(), minted());
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
async fn test_an_import_whose_identifier_cannot_be_minted_answers_operation_failed() {
    let (entropy, ctrl) = Entropy::new_mocked();
    ctrl.fail_next();
    // The identifier is minted before any connection is asked for.
    let store = PgStore::new(unreachable_db(), entropy);
    let record = Record {
        fleet: minted(),
        key: "copied".to_owned(),
        content: "what the source held".to_owned(),
        category: PINNED_CATEGORY.to_owned(),
        visibility: Visibility::Fleet,
        created_at_ms: AT,
        updated_at_ms: AT,
    };

    let refused = store
        .import(&minted(), &record)
        .await
        .expect_err("an import with no identifier to write under is refused");

    assert_eq!(
        (refused.code(), refused.detail()),
        (INTERNAL_OPERATION_FAILED, OPERATION_FAILED)
    );
}

#[tokio::test]
async fn test_a_capture_whose_grants_cannot_be_read_reports_the_datastore_refusal() {
    let memories = Memories::over(unreachable_db(), Arc::new(InMemory::new("unread")));
    let pushed = [delta("never_stored", PINNED_CATEGORY, Visibility::Fleet)];

    let refused = memories
        .capture(&minted(), &pushed, UnixMillis::from_millis(AT))
        .await
        .expect_err("a capture whose grants cannot be read is refused");

    assert!(refused.is_datastore_unavailable(), "{refused}");
    assert_eq!(refused.code(), INTERNAL_DB_UNAVAILABLE);
}
