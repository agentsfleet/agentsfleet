//! Old admitted work progresses through native assignments during continuous hot-fleet notifications.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{ACTOR, EVENT_TYPE, REQUEST_JSON, Scenario, scenario_with},
    e2e_native::Stored,
    e2e_seed, wire,
};
use af_core::{id::Uuid7, timing::ASSIGN_FALLBACK_TICK};
use afd_db::{
    Db, Migrator,
    config::DbRole,
    test_util::{FleetSeed, TenantSeed, TestDatabase, WalletSeed, WorkspaceSeed, mint_id},
};
use afd_fleet_lifecycle::FleetStatus;
use afd_turns::{IncomingTurn, Key, Producer, Reply, Turns};
use agentsfleetd::supervisor::Supervisor;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

const FLEETS: usize = 1_000;
const HOT_TICK: Duration = Duration::from_millis(10);
mod measurement;
use measurement::{Witness, assigned, probes, progress, wait_for_probes, zero_usage};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_thousand_eligible_fleets_make_old_work_progress_while_hot_notifications_continue() {
    let mut supervisor = Supervisor::new();
    let (database, owner, scene) = setup(&mut supervisor).await;
    let population = Population::seed(&scene, FLEETS).await;
    wire::heartbeat(&scene, &wire::capable_beat()).await;
    let first = assigned(&scene).await;
    assert_eq!(
        first.fleet, population.hot.fleet,
        "only the warm-up fleet is initially runnable"
    );
    population.activate(&scene, 1_000).await;
    let before = probes(&owner).await;
    population.hot.admit().await;
    let stop = CancellationToken::new();
    let _stop_on_exit = stop.clone().drop_guard();
    let produced = Arc::new(AtomicU64::new(0));
    let traffic = population.hot.produce(stop.clone(), Arc::clone(&produced));
    let mut witness = Witness::default();
    let measured = tokio::time::timeout(
        ASSIGN_FALLBACK_TICK * 3,
        progress(&scene, &owner, &population.hot, first, before, &mut witness),
    )
    .await;
    stop.cancel();
    traffic.await.expect("the hot producer joins");
    assert!(
        measured.is_ok(),
        "old unhinted work and the independent probe must progress under hot notifications: {witness:?}; matching_probes={}",
        probes(&owner).await.saturating_sub(before)
    );
    assert!(
        produced.load(Ordering::Relaxed) > 2,
        "the producer ran throughout the measurement"
    );
    cleanup(scene, owner, database, supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_busy_runner_remembers_recovery_when_its_worker_becomes_free() {
    let mut supervisor = Supervisor::new();
    let (database, owner, scene) = setup(&mut supervisor).await;
    let population = Population::seed(&scene, 2).await;
    wire::heartbeat(&scene, &wire::capable_beat()).await;
    let first = assigned(&scene).await;
    assert_eq!(first.fleet, population.hot.fleet);
    population.activate(&scene, 2).await;
    let before = probes(&owner).await;
    tokio::time::timeout(
        ASSIGN_FALLBACK_TICK * 3,
        wait_for_probes(&owner, before + 2),
    )
    .await
    .expect("the independent clock probes while the sole worker is busy");
    population.hot.admit().await;
    wire::report(&scene, &zero_usage(&first))
        .await
        .expect("the worker finishes");
    let next = assigned(&scene).await;
    assert_eq!(
        next.fleet, scene.fleet,
        "returning capacity must serve the older unhinted turn"
    );
    wire::report(&scene, &zero_usage(&next))
        .await
        .expect("the old turn settles");
    cleanup(scene, owner, database, supervisor).await;
}

async fn cleanup(mut scene: Scenario, owner: Db, database: TestDatabase, supervisor: Supervisor) {
    scene
        .native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    scene.cleanup().await;
    owner.close().await;
    database.cleanup().await;
}

async fn setup(supervisor: &mut Supervisor) -> (TestDatabase, Db, Scenario) {
    // Statement counts must identify this replica, so this case owns its migrated database.
    let database = TestDatabase::create().await;
    let owner = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&owner)
        .await
        .expect("the real schema migrates");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut *owner.acquire().await.expect("an owner connection"))
        .await
        .expect("actual statement counters are available");
    let api = database.api_url();
    let scene = scenario_with(
        supervisor,
        None,
        &[
            ("DATABASE_URL_API", api.as_str()),
            (afd_bus::wake::ENV_WAKE_QUEUE_CAPACITY, "1"),
        ],
        e2e_seed::FLEET_CONFIG_JSON,
    )
    .await;
    (database, owner, scene)
}

struct Population {
    fleets: Vec<Uuid7>,
    hot: Hot,
}

impl Population {
    async fn seed(scene: &Scenario, count: usize) -> Self {
        // Parked as `archived` (not leasable, and nothing sweeps it) until the
        // population is ready, then flipped to `active` by statement below.
        set_status(
            &scene.booted.database,
            std::slice::from_ref(&scene.fleet),
            FleetStatus::Archived,
        )
        .await;
        let common = e2e_seed::scenario_tag(&scene.fleet);
        let mut fleets = vec![scene.fleet.clone()];
        let mut hot = None;
        for number in 1..count {
            let (fleet, workspace) = seed_fleet(scene, &common).await;
            let source = Hot {
                turns: Turns::for_tests(scene.booted.database.clone(), scene.booted.bus.clone()),
                fleet: fleet.clone(),
                workspace,
            };
            source.admit().await;
            fleets.push(fleet);
            if number == count - 1 {
                hot = Some(source);
            }
        }
        let hot = hot.expect("the population contains its hot fleet");
        set_status(
            &scene.booted.database,
            std::slice::from_ref(&hot.fleet),
            FleetStatus::Active,
        )
        .await;
        Self { fleets, hot }
    }

    async fn activate(&self, scene: &Scenario, expected: i64) {
        // Make cold work eligible without publishing any new locality hint for it.
        set_status(&scene.booted.database, &self.fleets, FleetStatus::Active).await;
        let common = vec![e2e_seed::scenario_tag(&scene.fleet)];
        let eligible: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE id = ANY($1) AND status = $2 AND required_tags = $3")
            .bind(&self.fleets).bind(FleetStatus::Active.as_str()).bind(&common)
            .fetch_one(&mut *scene.booted.database.acquire().await.expect("population connection"))
            .await.expect("count the exact common-tag population");
        assert_eq!(eligible, expected);
    }
}

async fn seed_fleet(scene: &Scenario, common: &str) -> (Uuid7, Uuid7) {
    let (fleet, workspace, tenant) = (mint_id(), mint_id(), mint_id());
    let now = af_core::clock::now();
    let stored = Stored::of(&crate::e2e_native::definition(
        e2e_seed::FLEET_CONFIG_JSON,
        e2e_seed::FIXTURE_INSTRUCTIONS,
    ));
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("seed connection");
    TenantSeed::new(&tenant)
        .name("Code review")
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("tenant");
    WorkspaceSeed::new(&workspace, &tenant)
        .name("Engineering")
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("workspace");
    FleetSeed::new(&fleet, &workspace, &tenant, FleetStatus::Archived.as_str())
        .name("Pull request review")
        .tags(&[common])
        .at(now)
        .setup(stored.revision())
        .insert(&mut *connection)
        .await
        .expect("real placeable fleet");
    WalletSeed::new(&tenant, e2e_seed::DEEP_POOL)
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("funded tenant");
    (fleet, workspace)
}

async fn set_status(database: &Db, fleets: &[Uuid7], status: FleetStatus) {
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = ANY($1)")
        .bind(fleets)
        .bind(status.as_str())
        .execute(&mut *database.acquire().await.expect("status connection"))
        .await
        .expect("this fixture owns every selected fleet");
}

#[derive(Clone)]
struct Hot {
    turns: Turns,
    fleet: Uuid7,
    workspace: Uuid7,
}

impl Hot {
    async fn admit(&self) {
        self.turns
            .admit(IncomingTurn {
                producer: Producer::Steer,
                key: Key::Unrepeatable,
                fleet: &self.fleet,
                workspace: &self.workspace,
                actor: ACTOR,
                event_type: EVENT_TYPE,
                request_json: REQUEST_JSON,
                reply: Reply::None,
            })
            .await
            .expect("hot work commits and publishes its real broker wake");
        self.turns
            .streams()
            .publish(
                &afd_bus::Subject::ThreadWake(self.fleet.clone()),
                self.fleet.as_str(),
            )
            .await
            .expect("the real broker accepts continuous hot notifications");
    }

    fn produce(&self, stop: CancellationToken, produced: Arc<AtomicU64>) -> AbortOnDropHandle<()> {
        let hot = self.clone();
        AbortOnDropHandle::new(tokio::spawn(async move {
            let mut tick = tokio::time::interval(HOT_TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    () = stop.cancelled() => return,
                    _ = tick.tick() => { hot.admit().await; produced.fetch_add(1, Ordering::Relaxed); }
                }
            }
        }))
    }
}
