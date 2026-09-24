//! Two first mentions binding one channel at once, interleaved on purpose.
//!
//! Split from `resident_bound.rs`: the cases there arrange a binding that is
//! already committed, and this one holds it open so the statement that binds
//! has to wait on it.

#![cfg(feature = "test-util")]

use afd_db::test_util::mint_id;
use afd_ingress::slack::KIND_RESIDENT;

use super::resident_bound::{resident_document, resident_name};
use super::*;

/// A bind that waited on a concurrent first mention's uncommitted binding
/// answers the fleet that mention bound, not its own and not an error.
///
/// The one interleaving the insert-once statement cannot read back itself: it
/// blocks on the other transaction's row, finds it committed, writes nothing,
/// and its own read-back was taken before that commit. Held open here rather
/// than raced, and released only once Postgres reports the bind waiting on it,
/// so the branch runs every time.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_bind_that_waited_on_a_concurrent_binding_answers_it() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let name = resident_name(&fixture);
    let winner = fixture
        .fleet(&resident_document(&name), FleetStatus::Active.as_str())
        .await;
    let loser = fixture
        .fleet(
            &document("latecomer", OTHER_CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;

    let mut holder = fixture.database().acquire().await.expect("a connection");
    let mut open = sqlx::Acquire::begin(&mut *holder)
        .await
        .expect("the concurrent first mention's transaction opens");
    sqlx::query(
        "INSERT INTO core.connector_channels \
           (id, provider, external_account_id, external_channel_id, fleet_id, kind, created_at) \
         VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, 1)",
    )
    .bind(mint_id())
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(CHANNEL)
    .bind(winner.as_str())
    .bind(KIND_RESIDENT)
    .execute(&mut *open)
    .await
    .expect("the concurrent binding is written, not yet committed");

    let ingress = fixture.ingress();
    let workspace = fixture.workspace().clone();
    let team = fixture.team.clone();
    let channel: afd_ingress::slack::ChannelId = CHANNEL.parse().expect("a channel id");
    let bind = tokio::spawn(async move {
        ingress
            .bind_resident(
                &workspace,
                PROVIDER.id(),
                &team,
                &channel,
                &loser,
                afd_core::clock::UnixMillis::from_millis(1),
            )
            .await
    });
    assert!(
        waiting_on_a_lock(&fixture).await,
        "the bind never waited on the concurrent binding"
    );
    open.commit().await.expect("the concurrent binding commits");

    let bound = bind
        .await
        .expect("the bind task completes")
        .expect("the bind answers");
    assert_eq!(
        bound, winner,
        "the waiting bind answers the committed winner"
    );
    assert_eq!(
        bound_fleets(&fixture).await,
        1,
        "and wrote no second binding"
    );

    fixture.cleanup().await;
}

/// A second channel, so the latecomer's own document names somewhere else.
const OTHER_CHANNEL: &str = "C0987654321";

/// Whether the bind statement came to wait on a row lock held by another,
/// polled until it does or two seconds pass.
async fn waiting_on_a_lock(fixture: &Fixture) -> bool {
    for _attempt in 0..200 {
        let mut connection = fixture.database().acquire().await.expect("a connection");
        let waiting: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND query LIKE '%INSERT INTO core.connector_channels AS c%'",
        )
        .fetch_one(&mut *connection)
        .await
        .expect("the activity view reads");
        if waiting > 0 {
            return true;
        }
        drop(connection);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    false
}

/// How many bindings [`CHANNEL`] holds in the fixture's team.
async fn bound_fleets(fixture: &Fixture) -> i64 {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM core.connector_channels \
         WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3",
    )
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(CHANNEL)
    .fetch_one(&mut *connection)
    .await
    .expect("the bindings count")
}
