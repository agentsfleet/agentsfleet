//! Transaction start order must not move new work ahead of an unfinished run.

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_bus::wake::Wake;
use afd_fleet::run::{Acquired, Kind, RunService, Runs};
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};
use sqlx::Connection as _;
use tokio_util::sync::CancellationToken;

use super::{WITHIN, enqueue, next, reassignment::issue};
use crate::integration_run_gates::seed::seed_provider_resolution;
use crate::requests::placement_tag;
use crate::seed::select_fleet;
use crate::{rows::Row, seed::seeded_parts, support::Fixtures};

const ORDER: &str = "SELECT assigned.sequence < delayed.sequence \
    AND delayed.created_at < assigned.created_at \
    FROM thread.turns assigned, thread.turns delayed \
    WHERE assigned.id=$1 AND delayed.id=$2";

// Value: protects=unfinished work survives an older transaction committing later;
// fails_when=created_at outranks serialized insertion order; why_new=real producer
// transactions invert timestamp order; seam=Postgres plus turn and run stores.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_late_committing_turn_does_not_displace_the_assigned_head() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(&fixtures).await;
    let turns = Turns::for_tests(fixtures.database.clone(), afd_bus::test_util::unreachable());
    let delayed = turns
        .prepare(input(&fleet, &workspace))
        .await
        .expect("prepare");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("older transaction");
    let mut tx = connection.begin().await.expect("start before the winner");
    // Separate the transactions' millisecond timestamps before the second starts.
    sqlx::query("SELECT pg_sleep(0.002)")
        .execute(&mut *tx)
        .await
        .expect("clock tick");
    let assigned = turns
        .admit(input(&fleet, &workspace))
        .await
        .expect("newer commits first");
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &holder).await.expect("first assignment");
    assert_eq!(first.turn_id, assigned.stored.id);
    let issued = issue(&fixtures, &runs, &first, &holder, &tenant).await;
    let delayed = turns
        .stage(&mut tx, delayed)
        .await
        .expect("older inserts second");
    tx.commit().await.expect("late commit");
    let inverted: bool = sqlx::query_scalar(ORDER)
        .bind(&assigned.stored.id)
        .bind(&delayed.stored.id)
        .fetch_one(&mut *connection)
        .await
        .expect("actual transaction order");
    assert!(inverted, "the old query would select the new turn");
    fixtures.lapse_holder(&issued, &fleet).await;
    assert_reassigned(&fixtures, &runs, &poller, &first, &issued).await;
    drop((connection, runs, turns));
    fixtures.cleanup().await;
}

async fn assert_reassigned(
    fixtures: &Fixtures,
    runs: &Runs,
    poller: &Uuid7,
    first: &Acquired,
    issued: &Uuid7,
) {
    let recovered = next(runs, poller).await.expect("recover unfinished run");
    assert_eq!(recovered.turn_id, first.turn_id);
    assert!(recovered.redelivered);
    assert!(recovered.fence > first.fence);
    assert_eq!(recovered.kind, Kind::Reassigned);
    assert_eq!(
        fixtures.text(Row::Run(issued), "status").await.as_deref(),
        Some(afd_state::sql::RUN_STATUS_EXPIRED)
    );
}

// Value: protects=a turn whose issue was refused is the next pass's hint, ahead
// of a newer turn from the fleet the runner last ran; fails_when=a lost
// assignment rings no wake, so the released fleet sorts behind every fleet the
// runner ran; seam=the gate chain's refused issue, the wake over the lane's
// NATS and the ledger selection, on live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_turn_whose_assignment_was_lost_is_offered_before_newer_sticky_work() {
    let fixtures = Fixtures::create().await;
    let (sticky, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let lost = afd_db::test_util::mint_id();
    fixtures
        .seed_fleet(&lost, &workspace, &tenant, &placement_tag(&sticky))
        .await;
    let first = enqueue(&fixtures, &sticky, &workspace).await;
    let refused = enqueue(&fixtures, &lost, &workspace).await;
    let _default = seed_provider_resolution(&fixtures, &lost).await;
    // A namespace of its own, so this test's wake reaches no other test.
    let namespace = format!("lost-{}", afd_db::test_util::mint_id());
    let bus = afd_bus::test_util::lane_in(&namespace)
        .await
        .expect("the lane's NATS: run through make test-integration-rustd");
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let listening = wake
        .listen(&bus, &stop)
        .await
        .expect("the wake subscription opens");
    let runs = Runs::new(fixtures.database.clone(), bus, Entropy::new()).with_wake(wake.clone());
    let plane = RunService {
        runs: runs.clone(),
        ..fixtures.plane()
    };
    // The sticky fleet's run fills the one worker, so the issue refuses the other.
    let filling = select_fleet(&runs, &runner, &sticky)
        .await
        .expect("the sticky fleet is leasable");
    issue(&fixtures, &runs, &filling, &runner, &tenant).await;
    let assigned = select_fleet(&runs, &runner, &lost)
        .await
        .expect("the lost fleet is leasable");
    let mut rings = wake.generations();
    let answer = plane
        .assign_selected(assigned, &runner, fixtures.enrolled_at())
        .await
        .expect("a refused issue is a decision");
    let notified = tokio::time::timeout(WITHIN, rings.wait_for(|seen| seen.rings() > 0))
        .await
        .is_ok();
    end_run(&fixtures, &sticky, &first).await;
    let newer = enqueue(&fixtures, &sticky, &workspace).await;

    let offered = next(&runs, &runner).await.expect("both fleets wait");

    assert_eq!(
        (answer, notified, offered.turn_id),
        (None, true, refused),
        "the lost turn goes before the sticky fleet's newer turn {newer}"
    );
    stop.cancel();
    drop((listening, runs, plane));
    fixtures.cleanup().await;
}

/// Ends `fleet`'s run on `turn` and frees its worker and thread, leaving the
/// runner as the thread's last, as a processed report leaves it.
async fn end_run(fixtures: &Fixtures, fleet: &Uuid7, turn: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("runtime connection");
    sqlx::query("UPDATE thread.turns SET settled_at = clock_timestamp() WHERE id = $1")
        .bind(turn)
        .execute(&mut *connection)
        .await
        .expect("the turn settles");
    sqlx::query("UPDATE thread.runs SET status = $2 WHERE turn_id = $1 AND status = $3")
        .bind(turn)
        .bind(afd_state::sql::RUN_STATUS_EXPIRED)
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .execute(&mut *connection)
        .await
        .expect("the run ends");
    sqlx::query(
        "UPDATE thread.threads SET run_expires_at = clock_timestamp() - interval '1 second' \
         WHERE fleet_id = $1",
    )
    .bind(fleet)
    .execute(&mut *connection)
    .await
    .expect("the thread is free");
}

const fn input<'a>(fleet: &'a Uuid7, workspace: &'a Uuid7) -> IncomingTurn<'a> {
    IncomingTurn {
        producer: Producer::Steer,
        key: Key::Unrepeatable,
        fleet,
        workspace,
        actor: crate::seed::ACTOR,
        event_type: af_wire::turn::TurnKind::Chat,
        request_json: crate::seed::REQUEST_JSON,
        reply: Reply::None,
    }
}

/// How long a send may take to start waiting on the held fleet lock.
const BLOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Sessions waiting on `holder`, read from a separate autocommit connection (SQL-STATACT).
const BLOCKED_ON: &str =
    "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

/// A send that waits behind a staged turn's fleet lock draws the later
/// sequence, and the assignment takes the staged row first.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_send_queued_behind_the_fleet_lock_takes_the_later_sequence() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, _tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let turns = Turns::for_tests(fixtures.database.clone(), afd_bus::test_util::unreachable());
    let prepared = turns
        .prepare(input(&fleet, &workspace))
        .await
        .expect("prepare");
    let mut connection = fixtures.database.acquire().await.expect("holder");
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("holder backend");
    let mut tx = connection
        .begin()
        .await
        .expect("open the staging transaction");
    let staged = turns
        .stage(&mut tx, prepared)
        .await
        .expect("stage under the open transaction");
    let queued_send = tokio::spawn({
        let (turns, fleet, workspace) = (turns.clone(), fleet.clone(), workspace.clone());
        async move { turns.admit(input(&fleet, &workspace)).await }
    });
    let deadline = tokio::time::Instant::now() + BLOCK_WAIT;
    loop {
        let blocked: i64 = sqlx::query_scalar(BLOCKED_ON)
            .bind(holder)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("blocked sessions");
        if blocked > 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the send never waited on the fleet lock"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tx.commit().await.expect("the staged turn commits");
    let behind = queued_send
        .await
        .expect("the waiting send's task")
        .expect("the waiting send admits");
    let later: bool = sqlx::query_scalar(
        "SELECT w.sequence > s.sequence FROM thread.turns w, thread.turns s \
         WHERE w.id = $1 AND s.id = $2",
    )
    .bind(&behind.stored.id)
    .bind(&staged.stored.id)
    .fetch_one(&mut *connection)
    .await
    .expect("both sequences");
    assert!(later, "the waiter draws the later sequence");
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &runner).await.expect("the assignment");
    assert_eq!(
        first.turn_id, staged.stored.id,
        "the staged row is assigned first"
    );
}
