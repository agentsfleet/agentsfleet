//! A gate answer committed during a lock wait must prevent the delayed park.

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use sqlx::Connection as _;

use super::{WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const HOLD: &str = "SELECT pg_backend_pid() FROM runner.fleet_slots
    WHERE fleet_id = $1::uuid FOR UPDATE";
/// Sessions of this private database waiting on a lock. A second waiter on one
/// row queues behind the first waiter rather than the holder, so the holder's
/// own blocked list would never count it.
const WAITERS: &str = "SELECT count(*) FROM pg_stat_activity
    WHERE datname = current_database() AND wait_event_type = 'Lock'";
const GATE: &str = "INSERT INTO fleet.approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type,
     proposed_action, evidence, blast_radius, expires_at, status, detail,
     created_at, updated_at, event_id, spend_count, spend_ceiling)
    VALUES ($1::uuid, $2::uuid, $3::uuid, $1, 'git', 'push', 'repository_write',
      'open a pull request', '{}'::jsonb, 'one repository', now()+interval '1 hour',
      $4, '', now(), now(), NULL, 0, 32)";

// Value: protects=park and answer serialize on the fleet slot; fails_when=either
// slips past the other's slot lock and strands the head parked; why_new=the
// answer now takes the slot first, so both queue behind the held slot;
// seam=none; no_claim=which of the two wins the slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_answer_during_the_park_lock_wait_cannot_strand_the_admission() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let acquired = next(&leases, &runner).await.expect("claim");
    let action = seed_gate(&fixtures, &fleet, &workspace).await;
    let mut connection = fixtures.database.acquire().await.expect("blocker");
    let mut blocker = connection
        .begin()
        .await
        .expect("hold slot before admission lock");
    let _holder: i32 = sqlx::query_scalar(HOLD)
        .bind(&fleet)
        .fetch_one(&mut *blocker)
        .await
        .expect("fleet slot locked");
    let (parked, ()) = tokio::join!(
        leases.park(&acquired.fleet_id, &event, acquired.fence),
        answer_while_blocked(&fixtures, &action, blocker)
    );
    parked.expect("delayed park completes");
    let is_parked: bool =
        sqlx::query_scalar("SELECT parked_at IS NOT NULL FROM fleet.admissions WHERE id=$1::uuid")
            .bind(&event)
            .fetch_one(&mut *connection)
            .await
            .expect("park stamp");
    assert!(!is_parked, "the committed answer prevents a stale park");
    assert_eq!(
        next(&leases, &runner)
            .await
            .expect("answered head leases")
            .event_id,
        event
    );
    drop((connection, leases));
    fixtures.cleanup().await;
}

pub(super) async fn seed_gate(fixtures: &Fixtures, fleet: &str, workspace: &str) -> String {
    let action = afd_db::test_util::mint_id();
    sqlx::query(GATE)
        .bind(&action)
        .bind(fleet)
        .bind(workspace)
        .bind(afd_api_wire::approval::status::PENDING)
        .execute(&mut *fixtures.database.acquire().await.expect("seed connection"))
        .await
        .expect("pending runless gate");
    action
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stale_claim_cannot_park_or_release_its_replacement() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner, rival]) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let old = next(&leases, &runner).await.expect("first claim");
    seed_gate(&fixtures, &fleet, &workspace).await;
    fixtures
        .age(
            crate::rows::Row::Slot(&fleet),
            "lease_expires_at",
            crate::rows::LAPSE,
        )
        .await;
    let current = next(&leases, &rival).await.expect("replacement claim");
    assert!(current.fence.as_i64() > old.fence.as_i64());
    leases
        .park(&old.fleet_id, &event, old.fence)
        .await
        .expect("stale park is harmless");
    let state: (bool, Option<i64>, i64) = sqlx::query_as(
        "SELECT a.parked_at IS NOT NULL, a.lease_fence, s.fencing_token \
         FROM fleet.admissions a JOIN runner.fleet_slots s ON s.fleet_id=a.fleet_id \
         WHERE a.id=$1::uuid AND s.last_runner_id=$2::uuid",
    )
    .bind(&event)
    .bind(rival.as_str())
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the replacement still owns the slot");
    assert_eq!(
        state,
        (false, Some(current.fence.as_i64()), current.fence.as_i64())
    );
    leases
        .park(&current.fleet_id, &event, current.fence)
        .await
        .expect("the current holder can park");
    assert!(fixtures.slot_is_free(&fleet).await);
    drop(leases);
    fixtures.cleanup().await;
}

async fn answer_while_blocked(
    fixtures: &Fixtures,
    action: &str,
    blocker: sqlx::Transaction<'_, sqlx::Postgres>,
) {
    wait_for_waiters(fixtures, 1).await;
    let queue = crate::queue::unreachable();
    let inbox = Inbox::new(
        fixtures.database.clone(),
        queue.clone(),
        Admissions::for_tests(fixtures.database.clone(), queue),
    );
    let action = action.to_owned();
    let answer = tokio::spawn(async move {
        inbox
            .resolve(&action, Decision::Approved, "fixture:human", "", None)
            .await
    });
    // The answer queues behind the same slot rather than slipping past the
    // park: whichever then takes the slot first, the head ends unparked.
    wait_for_waiters(fixtures, 2).await;
    blocker.rollback().await.expect("release the slot");
    let answer = answer
        .await
        .expect("the answer joins")
        .expect("the answer commits once the slot is free");
    assert!(matches!(answer, Resolution::Resolved(_)));
}

/// Waits until `count` sessions queue on a lock.
async fn wait_for_waiters(fixtures: &Fixtures, count: i64) {
    let mut observer = fixtures.database.acquire().await.expect("observer");
    tokio::time::timeout(WITHIN, async {
        loop {
            let waiting: i64 = sqlx::query_scalar(WAITERS)
                .fetch_one(&mut *observer)
                .await
                .expect("observe the slot queue");
            if waiting >= count {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the writers queue behind the held slot");
}
