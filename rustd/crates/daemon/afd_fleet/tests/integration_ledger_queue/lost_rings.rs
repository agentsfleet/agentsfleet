//! A lost assignment rings its fleet's wake only when another pass can run
//! the turn: never for a host whose runner still offers a worker, nor for a
//! thread another holder took.

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_crypto::entropy::Entropy;
use af_observability::verb_span::VerbClock;
use afd_bus::wake::Wake;
use afd_fleet::run::{AssignRequest, RunService, Runs};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::{WITHIN, enqueue, reassignment::issue};
use crate::integration_run_gates::seed::seed_provider_resolution;
use crate::requests::placement_tag;
use crate::rows::{LAPSE, Row};
use crate::seed::{seeded_parts, select_fleet};
use crate::support::Fixtures;

/// One runner and a rival, a wake heard over the lane's NATS, and a plane
/// whose rings reach it.
struct Lane {
    fixtures: Fixtures,
    workspace: Uuid7,
    tenant: Uuid7,
    tag: String,
    runner: Uuid7,
    rival: Uuid7,
    bus: afd_bus::Handle,
    wake: Wake,
    runs: Runs,
    plane: RunService,
    _listening: JoinSet<()>,
    _stop: CancellationToken,
    _default: afd_db::test_util::PlatformDefault,
}

impl Lane {
    async fn open() -> Self {
        let fixtures = Fixtures::create().await;
        let (first, workspace, tenant, [runner, rival]) = seeded_parts::<2>(&fixtures).await;
        let default = seed_provider_resolution(&fixtures, &first).await;
        // A namespace of its own, so this test's wakes reach no other test.
        let namespace = format!("rings-{}", afd_db::test_util::mint_id());
        let bus = afd_bus::test_util::lane_in(&namespace)
            .await
            .expect("the lane's NATS: run through make test-integration-rustd");
        let (wake, stop) = (Wake::default(), CancellationToken::new());
        let listening = wake
            .listen(&bus, &stop)
            .await
            .expect("the wake subscription opens");
        let runs = Runs::new(fixtures.database.clone(), bus.clone(), Entropy::new())
            .with_wake(wake.clone());
        let plane = RunService {
            runs: runs.clone(),
            ..fixtures.plane()
        };
        Self {
            tag: placement_tag(&first),
            fixtures,
            workspace,
            tenant,
            runner,
            rival,
            bus,
            wake,
            runs,
            plane,
            _listening: listening,
            _stop: stop,
            _default: default,
        }
    }

    /// A fleet of this lane's runner with one chat turn waiting.
    async fn fleet(&self) -> Uuid7 {
        let fleet = afd_db::test_util::mint_id();
        self.fixtures
            .seed_fleet(&fleet, &self.workspace, &self.tenant, &self.tag)
            .await;
        enqueue(&self.fixtures, &fleet, &self.workspace).await;
        fleet
    }

    /// One pass for the runner as the gateway runs it, advertising `credits`.
    async fn pass(&self, credits: usize, ticket: Option<afd_runner::capacity::choice::Ticket>) {
        let answer = self
            .plane
            .assign_fleet_with(
                &mut AssignRequest {
                    runner: &self.runner,
                    held: &[],
                    degraded: false,
                    credits,
                    now: self.fixtures.enrolled_at(),
                    recovery: None,
                },
                ticket,
                &mut VerbClock::untimed(),
                |_run, _metadata| (),
            )
            .await
            .expect("a refused issue is a decision");
        assert!(answer.is_none(), "the pass issued no run");
    }

    /// Whether `fleet` was rung. A sentinel rung after the pass arrives after
    /// any ring the pass published on the same connection, so once it is heard
    /// the queue holds every ring the pass made.
    async fn rung(&self, fleet: &Uuid7) -> bool {
        let sentinel = afd_db::test_util::mint_id();
        afd_bus::wake::publish(&self.bus, sentinel.clone()).await;
        let mut generations = self.wake.generations();
        tokio::time::timeout(
            WITHIN,
            generations.wait_for(|_| self.wake.fleets().contains(&sentinel)),
        )
        .await
        .expect("the sentinel ring arrives")
        .expect("the wake stays open");
        self.wake.fleets().contains(fleet)
    }
}

// Value: protects=a full census while the runner still offers a worker rings
// no wake, so a pass cannot ring the same runner into the same refusal; a
// runner with no worker left still rings; fails_when=every full census rings;
// seam=the gateway's pass entry, the issue census and the lane's NATS.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_full_host_that_still_offers_a_worker_rings_no_wake() {
    let lane = Lane::open().await;
    let filling = lane.fleet().await;
    let held = select_fleet(&lane.runs, &lane.runner, &filling)
        .await
        .expect("the filling fleet is leasable");
    issue(
        &lane.fixtures,
        &lane.runs,
        &held,
        &lane.runner,
        &lane.tenant,
    )
    .await;
    let lost = lane.fleet().await;

    lane.pass(1, None).await;
    let disputed = lane.rung(&lost).await;
    lane.pass(0, None).await;
    let full = lane.rung(&lost).await;

    assert_eq!(
        (disputed, full),
        (false, true),
        "only a runner with no worker left rings"
    );
    lane.fixtures.cleanup().await;
}

// Value: protects=a moved placement choice and a lapsed assignment ring their
// fleet's wake, and a thread another runner holds does not; fails_when=the
// choice refusal goes unrung or untyped, or a superseded fence rings for a
// thread its holder serves; seam=the production pass and release over live
// Postgres and the lane's NATS.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_lost_assignment_rings_only_when_its_thread_is_free() {
    let lane = Lane::open().await;
    let moved = lane.fleet().await;
    let ticket = lane
        .fixtures
        .runners()
        .assignment_ticket(&lane.runner)
        .await
        .expect("the ticket reads");
    sqlx::query("UPDATE runner.runners SET run_version = run_version + 1 WHERE id = $1")
        .bind(&lane.runner)
        .execute(
            &mut *lane
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the placement choice moves");
    let log = Capture::install();
    lane.pass(1, ticket).await;
    let reason = log
        .only("run_assignment_lost")
        .field("reason")
        .map(str::to_owned);
    drop(log);
    let choice_rung = lane.rung(&moved).await;

    let lapsed = lane.fleet().await;
    let assigned = select_fleet(&lane.runs, &lane.runner, &lapsed)
        .await
        .expect("leasable");
    lane.fixtures
        .age(Row::Thread(&lapsed), "run_expires_at", LAPSE)
        .await;
    lane.plane
        .assign_selected(assigned, &lane.runner, lane.fixtures.enrolled_at())
        .await
        .expect("a lapsed issue is a decision");
    let lapsed_rung = lane.rung(&lapsed).await;

    let superseded = lane.fleet().await;
    let assigned = select_fleet(&lane.runs, &lane.runner, &superseded)
        .await
        .expect("leasable");
    lane.fixtures
        .age(Row::Thread(&superseded), "run_expires_at", LAPSE)
        .await;
    let won = lane
        .runs
        .assign(&superseded, &lane.rival, afd_runner::timing::RUN_TTL)
        .await
        .expect("the assignment statement answers");
    assert!(won.is_some(), "the rival takes the lapsed thread");
    lane.plane
        .assign_selected(assigned, &lane.runner, lane.fixtures.enrolled_at())
        .await
        .expect("a superseded issue is a decision");
    let superseded_rung = lane.rung(&superseded).await;

    assert_eq!(
        (reason.as_deref(), choice_rung, lapsed_rung, superseded_rung),
        (Some("choice_stale"), true, true, false)
    );
    lane.fixtures.cleanup().await;
}
