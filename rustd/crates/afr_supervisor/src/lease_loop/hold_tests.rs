#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use futures_util::FutureExt as _;

use crate::client::{Call, Verb};
use crate::error;
use crate::holds::Release;
use crate::test_support::{
    Answer, Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, Freezer, LEASE_ID, OUTCOME,
    PROCESSED, RENEWAL_TERMINATE, Rig, daemon, lease, reported,
};

/// The fleet's second lease, after the one that left its sandbox held.
pub(super) const NEXT_LEASE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// A fleet other than the one whose sandbox is held.
const OTHER_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a805b";
/// A fleet busy on a worker no lease in these tests runs on.
const BUSY_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a805c";
/// The event every release is logged under.
const RELEASED: &str = "sandbox_hold_released";
/// Where the report says the hold lapses: the rig's clock stands at zero.
const HELD_UNTIL: i64 = SANDBOX_HOLD_IDLE_MS;
/// The report field that carries it.
const HELD_UNTIL_MS: &str = "held_until_ms";

/// What the fake engine counted.
pub(super) struct Counted {
    prepared: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
    frozen: Arc<AtomicUsize>,
    thawed: Arc<AtomicUsize>,
}

impl Counted {
    fn of(engine: &FakeEngine) -> Self {
        Self {
            prepared: Arc::clone(&engine.prepared),
            destroyed: Arc::clone(&engine.destroyed),
            frozen: Arc::clone(&engine.frozen),
            thawed: Arc::clone(&engine.thawed),
        }
    }

    /// (prepared, frozen, thawed, destroyed)
    pub(super) fn read(&self) -> (usize, usize, usize, usize) {
        let load = |count: &AtomicUsize| count.load(Ordering::SeqCst);
        (
            load(&self.prepared),
            load(&self.frozen),
            load(&self.thawed),
            load(&self.destroyed),
        )
    }
}

pub(super) fn healthy(_call: &Call) -> Option<Answer> {
    None
}

/// A rig with two workers' worth of room for holds, so a processed lease
/// parks its sandbox.
pub(super) fn holding(
    special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
    engine: FakeEngine,
    behaviour: Behaviour,
) -> (Rig, Counted) {
    let counted = Counted::of(&engine);
    let rig = Rig::new(daemon(special), engine, FakeAgent::new(behaviour));
    rig.lessee.holds.resize(2);
    (rig, counted)
}

/// Waits until every release the registry was asked for is done, by asking
/// it to stop: the answer comes after each teardown.
pub(super) async fn settled(rig: &Rig) {
    rig.lessee.holds.shutdown().await;
}

/// Every release logged, as (fleet, reason), in the order they happened.
pub(super) fn releases(capture: &Capture) -> Vec<(String, String)> {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(RELEASED))
        .map(|event| {
            let field = |name| event.field(name).unwrap().to_owned();
            (field("fleet_id"), field("reason"))
        })
        .collect()
}

/// The release `releases` reads back for `fleet` ended for `reason`.
pub(super) fn released(fleet: &str, reason: Release) -> (String, String) {
    (fleet.to_owned(), reason.outcome().as_str().to_owned())
}

#[tokio::test(start_paused = true)]
async fn test_processed_lease_parks_its_sandbox_frozen() {
    let (mut rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[OUTCOME], PROCESSED);
    assert_eq!(report[HELD_UNTIL_MS], HELD_UNTIL);
    assert_eq!(
        counted.read(),
        (1, 1, 0, 0),
        "frozen and held, not destroyed"
    );
    let report_at = calls.iter().rposition(|call| call.verb == Verb::Report);
    let last_renew = calls.iter().rposition(|call| call.verb == Verb::Renew);
    assert!(
        last_renew < report_at,
        "a hold never extends the lease: no renewal after its report"
    );
}

#[tokio::test(start_paused = true)]
async fn test_next_lease_reuses_the_held_sandbox() {
    let (mut rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&lease(NEXT_LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(
        counted.read(),
        (1, 2, 1, 0),
        "one sandbox: built once, thawed for the second lease and held again"
    );
}

#[tokio::test(start_paused = true)]
async fn test_failed_lease_never_parks() {
    let (mut rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Break);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let report = reported(&rig.calls());
    assert_ne!(report[OUTCOME], PROCESSED);
    holds_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_an_interrupted_lease_never_parks() {
    let renewal_lost = |call: &Call| {
        (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
    };
    let (mut rig, counted) = holding(renewal_lost, FakeEngine::default(), Behaviour::Hang);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    holds_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_a_superseded_report_destroys_what_it_parked() {
    let superseded = |call: &Call| {
        (call.verb == Verb::Report).then(|| {
            Answer::Fail(error::refused(
                Verb::Report,
                409,
                Some(afd_core::error_code::RUN_STALE_FENCING_TOKEN),
            ))
        })
    };
    let (rig, counted) = holding(superseded, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    let held = rig.lessee.holds.fleets().await;
    settled(&rig).await;

    assert!(
        held.is_empty(),
        "released once the daemon refused the report"
    );
    assert_eq!(counted.read(), (1, 1, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_a_sandbox_that_will_not_freeze_is_destroyed() {
    let engine = FakeEngine {
        freezer: Freezer::RefusesFreeze,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    holds_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_thaw_failure_falls_back_fresh() {
    let engine = FakeEngine {
        freezer: Freezer::RefusesThaw,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&lease(NEXT_LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();
    settled(&rig).await;

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let (prepared, frozen, thawed, destroyed) = counted.read();
    assert_eq!(prepared, 2, "the second lease built a fresh sandbox");
    assert_eq!(frozen, 2, "and held it in turn");
    assert_eq!(thawed, 0, "no thaw succeeded");
    assert_eq!(
        destroyed, 2,
        "the hold that would not thaw, then shutdown's"
    );
}

#[tokio::test(start_paused = true)]
async fn test_another_fleets_lease_leaves_the_hold_alone() {
    let (rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&lease(NEXT_LEASE_ID, OTHER_FLEET, None))
        .await
        .unwrap();
    let held: Vec<String> = rig
        .lessee
        .holds
        .fleets()
        .await
        .iter()
        .map(|fleet| fleet.as_str().to_owned())
        .collect();

    assert_eq!(held, [FLEET_ID, OTHER_FLEET]);
    assert_eq!(counted.read(), (2, 2, 0, 0));
}

/// The lease that takes the runner's last free worker releases every other
/// fleet's hold as saturated, and rings the heartbeat so the daemon hears at
/// once; the fleet it serves parks in their place.
#[tokio::test(start_paused = true)]
async fn test_a_lease_on_the_last_free_worker_releases_other_holds() {
    let capture = Capture::install();
    let (rig, _counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    let _other_worker = rig.lessee.holds.occupy(Uuid7::parse(BUSY_FLEET).unwrap());

    rig.run(&lease(NEXT_LEASE_ID, OTHER_FLEET, None))
        .await
        .unwrap();
    let held = rig.lessee.holds.fleets().await;

    assert_eq!(held, [Uuid7::parse(OTHER_FLEET).unwrap()]);
    assert_eq!(releases(&capture), [released(FLEET_ID, Release::Saturated)]);
    let rung = rig.lessee.holds.saturated().notified().now_or_never();
    assert!(rung.is_some(), "the heartbeat is rung at once");
}

/// The report asks the daemon to hold nothing for its fleet.
fn holds_nothing(report: &serde_json::Value) {
    assert!(report.get(HELD_UNTIL_MS).is_none(), "{report}");
}
