//! Purge scope, authorization and rollback leave unrelated rows intact.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;

use crate::support::{Lane, NOW};

/// The resource version a seeded fleet starts at, which a kill moves past.
const SEEDED_VERSION: i64 = 1;

/// A tag the row has moved past is refused with the tag it holds now, and the
/// fleet survives.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_stale_tag_purges_nothing_and_answers_the_current_tag() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    let stale = afd_core::etag::versioned(&fleet, SEEDED_VERSION);
    let current = lane.kill(&fleet).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &fleet, &stale)
        .await
        .expect_err("a tag from before the kill must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_SOURCE_STALE);
    assert_eq!(refused.stale_tag(), Some(current.as_str()));
    assert!(
        lane.fleet_column(&fleet, "id").await.is_some(),
        "the fleet survives"
    );
    lane.cleanup().await;
}

/// A fleet that was never killed is refused even with its current tag.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_live_fleet_is_refused_until_it_is_killed() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    let tag = lane.tag(&fleet).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect_err("an active fleet must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_ALREADY_TERMINAL);
    assert_eq!(refused.stale_tag(), None);
    assert_eq!(
        lane.fleet_column(&fleet, "status").await.as_deref(),
        Some("active"),
        "the refused fleet is untouched"
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
    let tag = lane.kill(&theirs).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &theirs, &tag)
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
    let tag = lane.kill(&fleet).await;

    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
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
/// even though the lock and the tag both passed: the delete's own status
/// predicate is the last word. The delete is skipped by a trigger scoped to
/// this fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_whose_delete_removes_no_row_is_refused_and_rolled_back() {
    let lane = Lane::create().await;
    let fleet = lane.fleet().await;
    let tag = lane.kill(&fleet).await;
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
        .fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
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
    assert_eq!(refused.code(), error_code::AGENTSFLEET_ALREADY_TERMINAL);
    assert_eq!(
        lane.fleet_column(&fleet, "status").await.as_deref(),
        Some("killed"),
        "the refused purge left the fleet as it found it"
    );
    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect("with the delete free again, the same tag purges");
    assert!(
        lane.fleet_column(&fleet, "id").await.is_none(),
        "the fleet is gone"
    );
    lane.cleanup().await;
}
