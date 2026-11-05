//! The one-statement claim under contention, and what it reads back.

use std::sync::Arc;
use std::time::Duration;

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_approval::Decision;
use afd_db::config::DbRole;
use afd_dragonfly::doorbell::{Doorbell, WAKE_SHARDS, Waited};
use afd_dragonfly::{Dragonfly, FleetStreams, SubscriptionHub};
use afd_fleet::lease::Delivery;
use afd_fleet::lease::admit::Refusal;
use afd_wire::event::EventType;

use super::{admit_at, fence};
use crate::rows::Row;
use crate::seed::{claim_fleet, seeded_parts};
use crate::support::Fixtures;

/// Fleets in the Goal's estate besides the one that receives work.
const ESTATE: i32 = 999;
/// Beats each replica holds when the admission lands.
const HELD_PER_REPLICA: usize = 3;
/// Longer than any case waits, so a wait that ends early ended for a reason.
const LONG: Duration = Duration::from_secs(30);
/// Long enough for a woken beat to have answered.
const SETTLE: Duration = Duration::from_millis(200);
/// The claim's own text, by a phrase no other statement carries.
const CLAIM_CALLS: &str = "SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
     WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
       AND query LIKE '%replay_slot AS MATERIALIZED%'";
/// The seeded fleet's row, copied `$2` times.
const COPIES: &str = "INSERT INTO fleet.fleets \
    (id,workspace_id,tenant_id,name,status,required_tags,memory_reads_workspace, \
     memory_publishes_workspace,resource_version,created_at,updated_at) \
    SELECT uuidv7(),workspace_id,tenant_id,'estate-'||n,status,required_tags,false,false,1,now(),now() \
    FROM fleet.fleets CROSS JOIN generate_series(1,$2) n WHERE id=$1::uuid";
/// A fleet status the claim must pass over.
const STOPPED: &str = "stopped";
/// A refusal label the fixtures end an event with.
const ENDED: Refusal = Refusal::labelled("fixture_ended");

async fn claim_calls(fixtures: &Fixtures) -> i64 {
    sqlx::query_scalar(CLAIM_CALLS)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("an observer"))
        .await
        .expect("the statement counter reads")
}

/// Waits until every wake shard reports `wanted` subscribers: a ring sent
/// before both replicas listen would reach only one.
async fn subscribed(queue: &Dragonfly, wanted: i64) {
    let streams = FleetStreams::new(queue.clone());
    tokio::time::timeout(LONG, async {
        for shard in 0..WAKE_SHARDS {
            let channel = format!("fleet:wake:{{{shard}}}");
            while streams.publish(&channel, "probe").await.expect("probe") < wanted {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("both replicas subscribe");
}

/// Two daemon replicas, each a hub and a doorbell, with every held beat
/// already waiting, over a namespace of their own.
struct Replicas {
    queue: Dragonfly,
    hubs: [SubscriptionHub; 2],
    bells: [Doorbell; 2],
    waits: Vec<tokio::task::JoinHandle<Waited>>,
}

/// Starts [`Replicas`] for `fleet`. The namespace is the fleet's own: rings
/// name no fleet, and every test in this binary admits on the lane's shared
/// Dragonfly, so on the shared channels another test's admission would
/// complete these held beats too.
async fn replicas(fleet: &str) -> Replicas {
    let private = crate::queue::config()
        .with_namespace(&format!("goal-{fleet}"))
        .expect("a fleet id is a namespace");
    let queue = afd_dragonfly::test_util::connect_live(&private)
        .await
        .expect("the lane's Dragonfly");
    let hubs = [
        SubscriptionHub::start(private.clone())
            .await
            .expect("first replica"),
        SubscriptionHub::start(private)
            .await
            .expect("second replica"),
    ];
    let bells = [Doorbell::new(), Doorbell::new()];
    for (bell, hub) in bells.iter().zip(&hubs) {
        bell.listen(hub);
    }
    subscribed(&queue, 2).await;
    let mut waits = Vec::new();
    for bell in &bells {
        for _ in 0..HELD_PER_REPLICA {
            let bell = bell.clone();
            waits.push(tokio::spawn(
                async move { bell.wait_for_ring(LONG, 8).await },
            ));
        }
    }
    for bell in &bells {
        while bell.held() < HELD_PER_REPLICA {
            tokio::task::yield_now().await;
        }
    }
    Replicas {
        queue,
        hubs,
        bells,
        waits,
    }
}

/// One admission across a thousand active fleets and
/// two replicas completes at most one held beat on each, the winning poll
/// claims in one statement, and the losing poll's one statement writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_one_admission_claims_in_one_statement_and_wakes_one_beat_per_replica() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let (fleet, workspace, _, [winner, loser]) = seeded_parts::<2>(&fixtures).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut *connection)
        .await
        .expect("statement counter");
    sqlx::query(COPIES)
        .bind(&fleet)
        .bind(ESTATE)
        .execute(&mut *connection)
        .await
        .expect("the estate");
    drop(connection);
    owner.close().await;

    let Replicas {
        queue,
        hubs,
        bells,
        waits,
    } = replicas(&fleet).await;

    Admissions::for_tests(fixtures.database.clone(), queue)
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Repeated("goal-chat"),
            fleet: &fleet,
            workspace: &workspace,
            actor: crate::seed::ACTOR,
            event_type: EventType::Chat,
            request_json: "{\"message\":\"hello\"}",
            reply: Reply::None,
        })
        .await
        .expect("the chat is admitted and rings");
    tokio::time::sleep(SETTLE).await;
    let still_held: Vec<usize> = bells.iter().map(Doorbell::held).collect();

    let calls_before = claim_calls(&fixtures).await;
    let leases = fixtures.leases();
    let (won, cost) = leases.select_measured(&winner, &[]).await;
    let won = won
        .expect("the claim runs")
        .expect("the winner takes the chat");
    let lost = leases.select(&loser, &[]).await.expect("the claim runs");
    let calls = claim_calls(&fixtures).await - calls_before;
    for bell in &bells {
        bell.close();
    }
    let rung = futures_util::future::join_all(waits)
        .await
        .into_iter()
        .filter(|wait| matches!(wait, Ok(Waited::Rung)))
        .count();

    assert_eq!(
        still_held,
        vec![HELD_PER_REPLICA - 1; 2],
        "the admission completed exactly one held beat on each replica"
    );
    assert_eq!(rung, 2, "two beats rang in all, one per replica");
    assert_eq!(
        (
            won.fleet_id.as_str(),
            won.delivery,
            cost.database_roundtrips
        ),
        (fleet.as_str(), Delivery::First, 1),
        "the winning poll claimed and opened the event in one statement"
    );
    assert!(lost.is_none(), "the losing poll takes nothing: {lost:?}");
    assert_eq!(calls, 2, "one claim statement per poll");
    assert_eq!(
        fence(&fixtures, &fleet).await,
        Some(1),
        "the loser wrote nothing"
    );
    for hub in hubs {
        hub.shutdown();
    }
    fixtures.cleanup().await;
}

/// A fleet whose tags a runner lacks, and a fleet that is not
/// active, are claimed by no runner that may not run them.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_respects_required_tags_and_fleet_status() {
    let fixtures = Fixtures::create_with_queue().await;
    let (tagged, tagged_ws, _, [plain]) = seeded_parts::<1>(&fixtures).await;
    let (stopped, stopped_ws, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let gpu = format!("gpu-{tagged}");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query("UPDATE fleet.fleets SET required_tags = required_tags || $2 WHERE id = $1::uuid")
        .bind(&tagged)
        .bind(vec![gpu.clone()])
        .execute(&mut *connection)
        .await
        .expect("the fleet asks for a gpu");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(&stopped)
        .bind(STOPPED)
        .execute(&mut *connection)
        .await
        .expect("the fleet stops");
    drop(connection);
    let _gpu_work = admit_at(&fixtures, &tagged, &tagged_ws, 0).await;
    let _stopped_work = admit_at(&fixtures, &stopped, &stopped_ws, 0).await;
    let leases = fixtures.leases();

    assert!(
        claim_fleet(&leases, &plain, &tagged).await.is_none(),
        "a runner without the gpu tag passes over the gpu fleet"
    );
    assert!(
        claim_fleet(&leases, &runner, &stopped).await.is_none(),
        "a stopped fleet's ready slot is claimed by nobody"
    );
    assert!(
        fixtures.slot_is_free(&tagged).await && fixtures.slot_is_free(&stopped).await,
        "neither slot was touched"
    );
    fixtures.cleanup().await;
}

/// A lapsed claim on an open event reads `Repeat`, redelivered;
/// on an event that already ended it reads `Terminal`, and the pass settles
/// it with no lease. The first delivery's instant stands throughout.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_reclaim_reports_repeat_and_terminal_correctly() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let leases = fixtures.leases();
    let first = claim_fleet(&leases, &runner, &fleet).await.expect("first");
    let delivered = fixtures
        .instant(Row::Admission(&event), "delivered_at")
        .await;
    assert_eq!(
        (first.delivery, first.redelivered),
        (Delivery::First, false)
    );

    fixtures
        .age(
            Row::Slot(&fleet),
            "lease_expires_at",
            afd_core::timing::LEASE_TTL,
        )
        .await;
    let repeat = claim_fleet(&leases, &runner, &fleet).await.expect("repeat");
    assert_eq!(
        (repeat.delivery, repeat.redelivered),
        (Delivery::Repeat, true)
    );

    sqlx::query("UPDATE fleet.events SET status = $3 WHERE fleet_id = $1::uuid AND event_id = $2")
        .bind(&fleet)
        .bind(&event)
        .bind(afd_core::event::status::PROCESSED)
        .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the event ended elsewhere");
    fixtures
        .age(
            Row::Slot(&fleet),
            "lease_expires_at",
            afd_core::timing::LEASE_TTL,
        )
        .await;
    let terminal = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("terminal");
    assert_eq!(terminal.delivery, Delivery::Terminal);
    let answer = fixtures
        .plane()
        .lease_claimed(terminal, &runner, fixtures.enrolled_at())
        .await
        .expect("a finished event is a decision");

    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert_eq!(
        (
            fixtures
                .value::<bool>(Row::Admission(&event), "settled_at IS NOT NULL")
                .await,
            fixtures
                .instant(Row::Admission(&event), "delivered_at")
                .await,
            fixtures.slot_is_free(&fleet).await,
        ),
        (Some(true), delivered, true),
        "settled with no lease, its first delivery instant kept, the slot free"
    );
    fixtures.cleanup().await;
}

/// Claims, endings, parks, gate resolutions and admissions over
/// ten fleets, released together, never deadlock. Every fleet carries a
/// standing card, so a park parks and a resolution has a head to unpark.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_claim_report_park_and_admit_never_deadlock() {
    const FLEETS: usize = 10;
    const EACH: usize = 50;
    const KINDS: usize = 5;
    let fixtures = Arc::new(Fixtures::create_isolated_with_queue().await);
    let mut seeded = Vec::new();
    for _ in 0..FLEETS {
        let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
        for offset in 0..3 {
            admit_at(&fixtures, &fleet, &workspace, offset).await;
        }
        let card = super::approvals::pending_card(
            &fixtures,
            &fleet,
            &workspace,
            None,
            Duration::from_secs(3600),
        )
        .await;
        seeded.push((fleet, workspace, runner, card));
    }
    let seeded = Arc::new(seeded);
    let barrier = Arc::new(tokio::sync::Barrier::new(EACH * KINDS));
    let mut tasks = Vec::new();
    for index in 0..EACH {
        for kind in 0..KINDS {
            let (fixtures, seeded, barrier) = (
                Arc::clone(&fixtures),
                Arc::clone(&seeded),
                Arc::clone(&barrier),
            );
            tasks.push(tokio::spawn(async move {
                let (fleet, workspace, runner, card) = seeded
                    .get(index % FLEETS)
                    .expect("a fleet per slot")
                    .clone();
                barrier.wait().await;
                contend(&fixtures, kind, &fleet, &workspace, &runner, &card, index).await
            }));
        }
    }
    let mut failures = Vec::new();
    for task in tasks {
        if let Err(failure) = task.await.expect("the task joins") {
            failures.push(failure);
        }
    }

    let deadlocks: Vec<_> = failures
        .iter()
        .filter(|failure| failure.contains("40P01") || failure.contains("deadlock"))
        .collect();
    assert!(deadlocks.is_empty(), "{deadlocks:?}");
    assert!(failures.is_empty(), "no writer faulted: {failures:?}");
    Arc::into_inner(fixtures)
        .expect("every task has finished")
        .cleanup()
        .await;
}

/// One contender: a claim, an ending of what it claimed, a park of what it
/// claimed, a gate resolution, or an admission.
async fn contend(
    fixtures: &Fixtures,
    kind: usize,
    fleet: &str,
    workspace: &str,
    runner: &afd_core::id::Uuid7,
    card: &str,
    index: usize,
) -> Result<(), String> {
    let leases = fixtures.leases();
    match kind {
        0 => leases
            .select(runner, &[])
            .await
            .map(drop)
            .map_err(|e| e.to_string()),
        1 => match leases
            .select(runner, &[])
            .await
            .map_err(|e| e.to_string())?
        {
            Some(claimed) => leases
                .block(&claimed, ENDED)
                .await
                .map(drop)
                .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        2 => match leases
            .select(runner, &[])
            .await
            .map_err(|e| e.to_string())?
        {
            Some(claimed) => leases
                .park(&claimed.fleet_id, &claimed.event_id, claimed.fence)
                .await
                .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        3 => super::approvals::inbox(fixtures)
            .resolve(card, Decision::Approved, "fixture:human", "", Some(fleet))
            .await
            .map(drop)
            .map_err(|e| e.to_string()),
        _ => {
            crate::queue::enqueue(
                &fixtures.database,
                fleet,
                workspace,
                crate::seed::ACTOR,
                crate::seed::EVENT_TYPE,
                crate::seed::REQUEST_JSON,
                afd_core::clock::after(
                    fixtures.enrolled_at(),
                    Duration::from_secs(10 + index as u64),
                ),
            )
            .await;
            Ok(())
        }
    }
}
