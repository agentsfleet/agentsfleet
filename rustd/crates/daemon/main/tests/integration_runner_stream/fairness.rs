//! Old admitted work progresses through native offers during continuous hot-fleet notifications.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{ACTOR, EVENT_TYPE, REQUEST_JSON, Scenario, scenario_with},
    e2e_native::Stored,
    e2e_seed, wire,
};
use af_core::{id::Uuid7, timing::OFFER_FALLBACK_TICK};
use af_wire::{lease::TaggedLease, protocol::Down};
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::{
    Db, Migrator,
    config::DbRole,
    test_util::{FleetSeed, TenantSeed, TestDatabase, WalletSeed, WorkspaceSeed, mint_id},
};
use afd_fleet_lifecycle::FleetStatus;
use agentsfleetd::supervisor::Supervisor;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

const FLEETS: usize = 1_000;
const HOT_TICK: Duration = Duration::from_millis(10);
const PROBES: &str = "SELECT calls FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
      AND query LIKE $1";
const PROBE_PATTERN: &str = "%SELECT 1 FROM fleet.admissions a%FROM authoring.drafts d%";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_thousand_eligible_fleets_make_old_work_progress_while_hot_notifications_continue() {
    let mut supervisor = Supervisor::new();
    let (database, owner, mut run) = setup(&mut supervisor).await;
    let population = Population::seed(&run).await;
    wire::heartbeat(&run, &wire::capable_beat()).await;
    let first = offered(&run).await;
    assert_eq!(
        first.fleet, population.hot.fleet,
        "only the warm-up fleet is initially runnable"
    );
    population.activate(&run).await;
    let before = probes(&owner).await;
    population.hot.admit().await;
    let stop = CancellationToken::new();
    let _stop_on_exit = stop.clone().drop_guard();
    let produced = Arc::new(AtomicU64::new(0));
    let traffic = population.hot.produce(stop.clone(), Arc::clone(&produced));
    let measured = tokio::time::timeout(
        OFFER_FALLBACK_TICK * 3,
        progress(&run, &owner, &population.hot, first, before),
    )
    .await;
    stop.cancel();
    traffic.await.expect("the hot producer joins");
    assert!(
        measured.is_ok(),
        "old unhinted work and the independent probe must progress under hot notifications"
    );
    assert!(
        produced.load(Ordering::Relaxed) > 2,
        "the producer ran throughout the measurement"
    );
    run.native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
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
    let run = scenario_with(
        supervisor,
        None,
        &[
            ("DATABASE_URL_API", api.as_str()),
            (afd_bus::wake::ENV_WAKE_QUEUE_CAPACITY, "1"),
        ],
        e2e_seed::FLEET_CONFIG_JSON,
    )
    .await;
    (database, owner, run)
}

struct Population {
    fleets: Vec<Uuid7>,
    hot: Hot,
}

impl Population {
    async fn seed(run: &Scenario) -> Self {
        set_status(
            &run.booted.database,
            std::slice::from_ref(&run.fleet),
            FleetStatus::Stopped,
        )
        .await;
        let common = e2e_seed::scenario_tag(&run.fleet);
        let mut fleets = vec![run.fleet.clone()];
        let mut hot = None;
        for number in 1..FLEETS {
            let (fleet, workspace) = seed_fleet(run, &common).await;
            let source = Hot {
                admissions: Admissions::for_tests(
                    run.booted.database.clone(),
                    run.booted.bus.clone(),
                ),
                fleet: fleet.clone(),
                workspace,
            };
            source.admit().await;
            fleets.push(fleet);
            if number == FLEETS - 1 {
                hot = Some(source);
            }
        }
        let hot = hot.expect("the population contains its hot fleet");
        set_status(
            &run.booted.database,
            std::slice::from_ref(&hot.fleet),
            FleetStatus::Active,
        )
        .await;
        Self { fleets, hot }
    }

    async fn activate(&self, run: &Scenario) {
        // Make cold work eligible without publishing any new locality hint for it.
        set_status(&run.booted.database, &self.fleets, FleetStatus::Active).await;
        let common = vec![e2e_seed::scenario_tag(&run.fleet)];
        let eligible: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE id = ANY($1) AND status = $2 AND required_tags = $3")
            .bind(&self.fleets).bind(FleetStatus::Active.as_str()).bind(&common)
            .fetch_one(&mut *run.booted.database.acquire().await.expect("population connection"))
            .await.expect("count the exact common-tag population");
        assert_eq!(eligible, 1_000);
    }
}

async fn seed_fleet(run: &Scenario, common: &str) -> (Uuid7, Uuid7) {
    let (fleet, workspace, tenant) = (mint_id(), mint_id(), mint_id());
    let now = af_core::clock::now();
    let stored = Stored::of(&crate::e2e_native::definition(
        e2e_seed::FLEET_CONFIG_JSON,
        e2e_seed::FIXTURE_INSTRUCTIONS,
    ));
    let mut connection = run
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
    FleetSeed::new(&fleet, &workspace, &tenant, FleetStatus::Stopped.as_str())
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
    admissions: Admissions,
    fleet: Uuid7,
    workspace: Uuid7,
}

impl Hot {
    async fn admit(&self) {
        self.admissions
            .admit(Admission {
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
        self.admissions
            .streams()
            .publish(&afd_bus::Subject::Wake(0), self.fleet.as_str())
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

struct Offered {
    fleet: Uuid7,
    event: Uuid7,
    lease: String,
    fence: u64,
}

async fn offered(run: &Scenario) -> Offered {
    let frame = tokio::time::timeout(
        OFFER_FALLBACK_TICK + af_core::timing::STREAM_ANSWER_TIMEOUT,
        run.native_session().await.offer(),
    )
    .await
    .expect("a ready worker receives a bounded native offer")
    .expect("native offer");
    let decoded = frame
        .decode::<Down<'_>>()
        .expect("the current offer schema");
    let lease = match decoded {
        Down::Offer {
            lease: TaggedLease::Fleet(lease),
        } => Some(lease),
        _ => None,
    }
    .expect("only fleet work was seeded");
    Offered {
        fleet: Uuid7::parse(&lease.event.fleet_id).expect("fleet id"),
        event: Uuid7::parse(&lease.event.event_id).expect("event id"),
        lease: lease.lease_id.to_string(),
        fence: lease.fencing_token,
    }
}

async fn progress(run: &Scenario, owner: &Db, hot: &Hot, first: Offered, before: u64) {
    let mut next = first;
    let mut old = false;
    let mut hot_reported = false;
    loop {
        // Ensure a hot hint and another eligible hot admission precede every capacity release.
        hot.admit().await;
        wire::report(run, &zero_usage(&next))
            .await
            .expect("the native report settles");
        old |= next.event == run.event_id;
        hot_reported |= next.fleet == hot.fleet;
        if old && hot_reported && probes(owner).await >= before + 2 {
            break;
        }
        next = offered(run).await;
    }
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1 AND fleet_id = $2",
    )
    .bind(&run.event_id)
    .bind(&run.fleet)
    .fetch_one(&mut *owner.acquire().await.expect("the result connection"))
    .await
    .expect("the original admission remains readable");
    assert!(
        settled,
        "older work reached its durable result while hot demand continued"
    );
}

fn zero_usage(offer: &Offered) -> serde_json::Value {
    let mut body = wire::report_body(&offer.lease, &offer.event, offer.fence);
    let fields = body
        .as_object_mut()
        .expect("the report fixture is an object");
    fields.insert("tokens".into(), serde_json::json!(0));
    fields.insert("requests".into(), serde_json::json!([]));
    body
}

async fn probes(database: &Db) -> u64 {
    let calls: Vec<i64> = sqlx::query_scalar(PROBES)
        .bind(PROBE_PATTERN)
        .fetch_all(
            &mut *database
                .acquire()
                .await
                .expect("statement counter connection"),
        )
        .await
        .expect("the real probe counter is readable");
    // A missing baseline is zero; the final assertion requires two actual matching executions.
    calls
        .into_iter()
        .map(|count| u64::try_from(count).expect("statement counts are nonnegative"))
        .sum()
}
