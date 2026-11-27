#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::clock::after;
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_core::timing::DAY;
use af_wire::run::RunPayload;
use af_wire::runner::NetworkPolicy;
use futures_util::FutureExt as _;
use jiff::Timestamp;

use crate::client::{Call, Verb};
use crate::error;
use crate::parks::Release;
use crate::test_support::ROUND;
use crate::test_support::{
    Answer, Behaviour, DESTROY, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, KILL, OUTCOME,
    PROCESSED, Parking, RENEWAL_TERMINATE, RUN_ID, Rig, assigned, bounded, daemon, payload,
    position, reported,
};

/// The fleet's second run, after the one that left its sandbox parked.
pub(super) const NEXT_RUN_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// A fleet other than the one whose sandbox is parked.
const OTHER_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a805b";
/// A fleet busy on a worker no run in these tests runs on.
const BUSY_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a805c";
/// The event every release is logged under.
const RELEASED: &str = "sandbox_park_released";
/// The report field that says when the park lapses.
const PARKED_UNTIL: &str = "parked_until";

/// What the fake engine counted.
pub(super) struct Counted {
    prepared: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
    parked: Arc<AtomicUsize>,
    resumed: Arc<AtomicUsize>,
}

impl Counted {
    pub(super) fn of(engine: &FakeEngine) -> Self {
        Self {
            prepared: Arc::clone(&engine.prepared),
            destroyed: Arc::clone(&engine.destroyed),
            parked: Arc::clone(&engine.parked),
            resumed: Arc::clone(&engine.resumed),
        }
    }

    /// The counts so far, as (prepared, parked, resumed, destroyed).
    pub(super) fn read(&self) -> (usize, usize, usize, usize) {
        let load = |count: &AtomicUsize| count.load(Ordering::SeqCst);
        (
            load(&self.prepared),
            load(&self.parked),
            load(&self.resumed),
            load(&self.destroyed),
        )
    }
}

pub(super) fn healthy(_call: &Call) -> Option<Answer> {
    None
}

/// A rig with two workers' worth of room for parks, so a processed run
/// parks its sandbox.
pub(super) fn parking_rig(
    special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
    engine: FakeEngine,
    behaviour: Behaviour,
) -> (Rig, Counted) {
    let counted = Counted::of(&engine);
    let rig = Rig::new(daemon(special), engine, FakeAgent::new(behaviour));
    rig.lessee.parks.resize(2);
    (rig, counted)
}

/// The fleet's next run, which the daemon says should resume the sandbox
/// this runner parks for it.
pub(super) fn resumed(run_id: &str, fleet: &str) -> RunPayload<'static> {
    let mut next = payload(run_id, fleet);
    next.resume_park = true;
    next
}

/// Waits until every release the registry was asked for is done, by asking
/// it to stop: the answer comes after each teardown.
pub(super) async fn settled(rig: &Rig) {
    bounded("the parks' shutdown", rig.lessee.parks.shutdown()).await;
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

/// The run parks its sandbox, then outlives two renewal ticks pushing its
/// memory, so renewal is proved running while the sandbox is parked; time then
/// runs past more ticks, and no renewal follows the report.
#[tokio::test(start_paused = true)]
async fn test_processed_run_parks_its_sandbox() {
    let reports = Arc::new(AtomicUsize::new(0));
    let attempts = Arc::clone(&reports);
    let slow_report = move |call: &Call| {
        if call.verb != Verb::Report {
            return None;
        }
        (attempts.fetch_add(1, Ordering::SeqCst) < 2).then_some(Answer::Stall)
    };
    let (mut rig, counted) = parking_rig(slow_report, FakeEngine::default(), Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    let calls = rig.calls();
    assert_eq!(reports.load(Ordering::SeqCst), 3);
    tokio::time::sleep(ROUND * 2).await;
    assert!(
        rig.calls().iter().all(|call| call.verb != Verb::Renew),
        "a park never renews after acknowledgement"
    );
    let report = reported(&calls);
    assert_eq!(report[OUTCOME], PROCESSED);
    // The rig's clock stands at zero, and the park asks for the most a daemon may grant.
    assert_eq!(
        report[PARKED_UNTIL],
        after(Timestamp::UNIX_EPOCH, DAY).to_string()
    );
    assert_eq!(
        counted.read(),
        (1, 1, 0, 0),
        "parked and kept, not destroyed"
    );
    let renewals: Vec<usize> = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| call.verb == Verb::Renew)
        .map(|(at, _)| at)
        .collect();
    assert!(!renewals.is_empty(), "the run outlived a renewal tick");
    assert!(position(&calls, Verb::Report).is_some());
}

/// A run refused before its sandbox is chosen, here by an egress host the
/// resolver cannot answer, settles with no park, so the daemon would never let
/// the fleet's parked sandbox be resumed after it: the refusal releases it at
/// once.
#[tokio::test(start_paused = true)]
async fn test_a_run_refused_at_startup_releases_the_fleets_park() {
    let capture = Capture::install();
    let (rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let refused = resumed(NEXT_RUN_ID, FLEET_ID);
    let unresolvable = assigned(NetworkPolicy::AllowListEgress, &["unknown.example"]);
    rig.run_under(&refused, &unresolvable).await.unwrap();
    settled(&rig).await;

    assert_eq!(releases(&capture), [released(FLEET_ID, Release::Mismatch)]);
    let (_prepared, _parked, _resumed, destroyed) = counted.read();
    assert_eq!(destroyed, 1, "the parked sandbox is torn down");
}

#[tokio::test(start_paused = true)]
async fn test_next_run_reuses_the_parked_sandbox() {
    let (mut rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(
        counted.read(),
        (1, 2, 1, 0),
        "one sandbox: built once, resumed for the second run and parked again"
    );
}

#[tokio::test(start_paused = true)]
async fn test_failed_run_never_parks() {
    let (mut rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Break);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_ne!(report[OUTCOME], PROCESSED);
    parks_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_an_interrupted_run_never_parks() {
    let renewal_lost = |call: &Call| {
        (call.verb == Verb::Renew).then(|| Answer::Fail(error::refused(Verb::Renew, 409, None)))
    };
    let (mut rig, counted) = parking_rig(renewal_lost, FakeEngine::default(), Behaviour::Hang);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    parks_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn test_a_superseded_report_destroys_what_it_parked() {
    let superseded = |call: &Call| {
        (call.verb == Verb::Report).then(|| {
            Answer::Fail(error::refused(
                Verb::Report,
                409,
                Some(af_core::error_code::RUN_STALE_FENCING_TOKEN),
            ))
        })
    };
    let (rig, counted) = parking_rig(superseded, FakeEngine::default(), Behaviour::Answer);

    let failure = rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap_err();
    assert_eq!(
        failure.refusal_code(),
        Some(af_core::error_code::RUN_STALE_FENCING_TOKEN)
    );
    let parked = rig.lessee.parks.fleets().await;
    settled(&rig).await;

    assert!(
        parked.is_empty(),
        "released once the daemon refused the report"
    );
    assert_eq!(counted.read(), (1, 1, 0, 1));
}

/// A sandbox that will not park is destroyed by its engine; the run parks
/// nothing and destroys nothing more.
#[tokio::test(start_paused = true)]
async fn test_a_sandbox_that_will_not_park_is_destroyed_by_its_engine() {
    let engine = FakeEngine {
        parking: Parking::RefusesPark,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = parking_rig(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    parks_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
}

/// An engine that offers no park mode kills a processed run's sandbox and
/// releases it after the report, parking nothing.
#[tokio::test(start_paused = true)]
async fn test_an_engine_that_cannot_park_kills_the_sandbox() {
    let (steps, mut taken) = tokio::sync::mpsc::unbounded_channel();
    let engine = FakeEngine {
        parking: Parking::Unoffered,
        steps: Some(steps),
        ..FakeEngine::default()
    };
    let (mut rig, counted) = parking_rig(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    parks_nothing(&report);
    assert_eq!(counted.read(), (1, 0, 0, 1));
    assert_eq!(taken.try_recv().unwrap(), KILL);
    assert_eq!(taken.try_recv().unwrap(), DESTROY);
}

#[tokio::test(start_paused = true)]
async fn test_another_fleets_run_leaves_the_park_alone() {
    let (rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.lessee.parks.resize(3);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&payload(NEXT_RUN_ID, OTHER_FLEET)).await.unwrap();
    let parked: Vec<String> = rig
        .lessee
        .parks
        .fleets()
        .await
        .iter()
        .map(|fleet| fleet.as_str().to_owned())
        .collect();

    assert_eq!(parked, [FLEET_ID, OTHER_FLEET]);
    assert_eq!(counted.read(), (2, 2, 0, 0));
}

/// The run that takes the runner's last free worker releases every other
/// fleet's park as saturated, and rings the heartbeat so the daemon hears at
/// once; the fleet it serves parks in their place.
#[tokio::test(start_paused = true)]
async fn test_a_run_on_the_last_free_worker_releases_other_parks() {
    let capture = Capture::install();
    let (rig, _counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.lessee.parks.resize(3);
    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    let _other_worker = rig.lessee.parks.occupy(crate::parks::Occupant {
        run: Uuid7::parse(BUSY_FLEET).unwrap(),
        fleet: Uuid7::parse(BUSY_FLEET).unwrap(),
    });

    rig.run(&payload(NEXT_RUN_ID, OTHER_FLEET)).await.unwrap();
    let parked = rig.lessee.parks.fleets().await;

    assert_eq!(parked, [Uuid7::parse(OTHER_FLEET).unwrap()]);
    assert_eq!(releases(&capture), [released(FLEET_ID, Release::Saturated)]);
    let rung = rig.lessee.parks.saturated().notified().now_or_never();
    assert!(rung.is_some(), "the heartbeat is rung at once");
}

/// The report asks the daemon to park nothing for its fleet.
pub(super) fn parks_nothing(report: &serde_json::Value) {
    assert!(report.get(PARKED_UNTIL).is_none(), "{report}");
}
