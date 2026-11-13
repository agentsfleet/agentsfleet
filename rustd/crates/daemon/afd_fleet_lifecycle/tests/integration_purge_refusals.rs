//! Purge scope, authorization and rollback leave unrelated rows intact.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code;

use crate::support::{Lane, NOW};

/// An active fleet purges under its current tag: delete and archive are
/// separate operations, so a delete takes any status.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn an_active_fleet_purges_without_an_archive_first() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;

    lane.delete(&lane.workspace, &fleet)
        .await
        .expect("an active fleet purges");

    assert!(
        lane.fleet_column(&fleet, "id").await.is_none(),
        "the fleet is gone"
    );
    lane.cleanup().await;
}

/// Another workspace's fleet answers as absent, never as its status.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn another_workspaces_fleet_is_not_found() {
    let lane = Lane::create().await;
    let neighbour = lane.another_workspace().await;
    let theirs = lane.fleet_at(&neighbour, NOW).await;
    lane.archive(&theirs).await;

    let refused = lane
        .delete(&lane.workspace, &theirs)
        .await
        .expect_err("a fleet outside the caller's workspace must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_NOT_FOUND);
    assert!(
        lane.fleet_column(&theirs, "id").await.is_some(),
        "their fleet survives"
    );
    lane.cleanup().await;
}

/// Purge commits its database deletion.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_commits_its_database_deletion() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    lane.archive(&fleet).await;

    lane.delete(&lane.workspace, &fleet)
        .await
        .expect("purge commits");

    assert!(
        lane.fleet_column(&fleet, "id").await.is_none(),
        "the row is gone"
    );
    lane.cleanup().await;
}

/// Serialises fixture DDL with every other suite's, so two trigger installs on
/// one table cannot deadlock; the value is the one the authoring fixture takes.
const FIXTURE_DDL_LOCK: i64 = 0x6166_645f_6974;

/// Runs `statements` as one transaction under [`FIXTURE_DDL_LOCK`], as the
/// lane's owner: a trigger on `fleet.fleets` is the table owner's to install.
async fn fixture_ddl(lane: &Lane, statements: [String; 2]) {
    let owner = lane.owner().await;
    let mut connection = owner.acquire().await.expect("an owner connection");
    let mut tx = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("a fixture DDL transaction");
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(FIXTURE_DDL_LOCK)
        .execute(&mut *tx)
        .await
        .expect("serialise fixture DDL");
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .expect("the fixture DDL applies");
    }
    tx.commit().await.expect("the fixture DDL commits");
    drop(connection);
    owner.close().await;
}

/// A purge whose parent delete removes no row is refused and commits nothing,
/// even though the lock and the tag both passed: the delete's own row count
/// is the last word. The delete is skipped by a trigger scoped to this fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_whose_delete_removes_no_row_is_refused_and_rolled_back() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    lane.archive(&fleet).await;
    // The identifier is minted, never input, which is what makes naming the
    // trigger after it and comparing against it in the DDL safe.
    let name = format!("afd_it_skip_{}", fleet.as_str().replace('-', ""));
    fixture_ddl(
        &lane,
        [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN RETURN NULL; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} BEFORE DELETE ON fleet.fleets FOR EACH ROW \
                 WHEN (OLD.id = '{fleet}'::uuid) EXECUTE FUNCTION public.{name}()"
            ),
        ],
    )
    .await;

    let refused = lane
        .delete(&lane.workspace, &fleet)
        .await
        .expect_err("a delete that removed nothing is not a purge");

    fixture_ddl(
        &lane,
        [
            format!("DROP TRIGGER IF EXISTS {name} ON fleet.fleets"),
            format!("DROP FUNCTION IF EXISTS public.{name}()"),
        ],
    )
    .await;
    assert_eq!(refused.code(), error_code::AGENTSFLEET_NOT_FOUND);
    assert_eq!(
        lane.fleet_column(&fleet, "status").await.as_deref(),
        Some("archived"),
        "the refused purge left the fleet as it found it"
    );
    lane.delete(&lane.workspace, &fleet)
        .await
        .expect("with the delete free again, the same tag purges");
    assert!(
        lane.fleet_column(&fleet, "id").await.is_none(),
        "the fleet is gone"
    );
    lane.cleanup().await;
}
