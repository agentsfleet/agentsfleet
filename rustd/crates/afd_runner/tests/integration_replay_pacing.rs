//! The replay sweeper's pacing against a real backlog: a batch the queue took
//! whole brings the next pass straight back, and the pass that finds the
//! backlog drained waits the ordinary interval again.
//!
//! The backlog is rows committed while the queue was away — the state replay
//! exists for — written through a ledger whose queue handle dials a closed
//! port, so the lane's own queue is never made to fail.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, FleetStreams};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::replay::Replay;
use afd_wire::event::EventType;

use crate::support::connect_redis;

/// Rows left for replay: more than one pass takes, so the first is full.
const BACKLOG: usize = 80;

/// Passes the backlog may take to drain, with room for the lane's own rows.
const MAX_PASSES: usize = 16;

/// A port nothing listens on, so every append through it fails at the socket.
const CLOSED_QUEUE: &str = "redis://127.0.0.1:1";

/// Who the history records as having woken the fleet.
const ACTOR: &str = "webhook:replay-pacing";

/// Rows on the fleet still waiting for a receipt.
const UNRECEIPTED: &str =
    "SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid AND receipt IS NULL";

fn admission<'a>(fleet: &'a str, workspace: &'a str, key: &'a str) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: r#"{"delivery":"replay-pacing"}"#,
        reply: Reply::None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_full_clean_batch_comes_straight_back_and_a_drained_one_waits() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let (tenant, workspace, fleet) = (mint_id(), mint_id(), mint_id());
    let mut connection = database.acquire().await.expect("an API connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("the tenant seeds");
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .expect("the workspace seeds");
    FleetSeed::new(&fleet, &workspace, &tenant, "active")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("the fleet seeds");
    drop(connection);

    let closed = DragonflyConfig::from_url(DragonflyRole::Default, CLOSED_QUEUE.to_owned())
        .with_request_timeout(Duration::from_millis(200));
    let deferring = Admissions::for_tests(
        database.clone(),
        Dragonfly::unreachable(&closed).expect("a closed queue handle builds"),
    );
    for index in 0..BACKLOG {
        let key = format!("replay-pacing-{fleet}-{index}");
        deferring
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("the row commits whatever the queue does");
    }

    let queue = connect_redis().await;
    let replay = Replay::for_rebuild(Admissions::for_tests(database.clone(), queue.clone()));
    let ordinary = replay.interval();
    let first = replay.sweep().await.expect("the replay pass runs");
    assert!(first.scanned > 0, "the backlog is due at once on a rebuild");
    assert_eq!(
        first.changed, first.scanned,
        "the queue took the whole batch"
    );
    assert_eq!(
        replay.interval(),
        Duration::ZERO,
        "a full batch the queue took whole means more is waiting"
    );

    let mut passes = 1;
    while replay.interval() == Duration::ZERO {
        replay.sweep().await.expect("the replay pass runs");
        passes += 1;
        assert!(passes <= MAX_PASSES, "the backlog drains");
    }
    assert_eq!(replay.interval(), ordinary, "a drained pass waits again");
    let waiting: i64 = sqlx::query_scalar(UNRECEIPTED)
        .bind(&fleet)
        .fetch_one(&mut *database.acquire().await.expect("an API connection"))
        .await
        .expect("the receipts read");
    assert_eq!(waiting, 0, "every deferred row has its receipt");

    FleetStreams::new(queue)
        .forget(&fleet)
        .await
        .expect("the fleet's stream is removed");
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
        .bind(&tenant)
        .execute(&mut *connection)
        .await
        .expect("the scoped fixture cleans up");
    drop(connection);
    lane.cleanup().await;
}
