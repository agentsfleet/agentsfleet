//! One admission under contention: one claim statement, one beat per replica.

use std::time::Duration;

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::config::DbRole;
use afd_dragonfly::doorbell::{Doorbell, WAKE_SHARDS, Waited};
use afd_dragonfly::{Dragonfly, FleetStreams, SubscriptionHub};
use afd_fleet::lease::Delivery;
use afd_wire::event::EventType;

use super::fence;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// Fleets in the Goal's estate besides the one that receives work.
const ESTATE: i32 = 999;
/// Beats each replica holds when the admission lands.
const HELD_PER_REPLICA: usize = 3;
/// Longer than any case waits, so a wait that ends early ended for a reason.
const LONG: Duration = Duration::from_secs(30);
/// How long the beat counts must hold still once the ring has landed, so a
/// second completion on a replica would have shown.
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
    // The ring lands when it lands: wait for one beat to end on each replica,
    // then hold still long enough for a second completion to have shown.
    tokio::time::timeout(LONG, async {
        while bells.iter().any(|bell| bell.held() >= HELD_PER_REPLICA) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the ring reached both replicas");
    // gstack-timing-ok: a quiet window after the outcome, not a race for it.
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
