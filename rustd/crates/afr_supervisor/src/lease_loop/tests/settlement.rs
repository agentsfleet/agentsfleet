#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::error_code::RUN_STALE_FENCING_TOKEN;
use afd_core::test_util::trace::Capture;

use crate::client::{Call, Verb};
use crate::error;
use crate::holds::Release;
use crate::report_spool::ReportSpool;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME, PROCESSED, Rig, daemon,
    lease, position, reported,
};

type Special = fn(&Call) -> Option<Answer>;

fn rig(special: Special) -> Rig {
    Rig::new(
        daemon(special),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    )
}

fn report_busy(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Report).then(|| Answer::Fail(error::unavailable(Verb::Report, 503)))
}

fn report_unauthorized(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Report).then(|| Answer::Fail(error::refused(Verb::Report, 401, None)))
}

/// Whether the drain was handed a held report.
async fn drain_rung(rig: &Rig) -> bool {
    tokio::time::timeout(Duration::ZERO, rig.lessee.held.notified())
        .await
        .is_ok()
}

#[tokio::test(start_paused = true)]
async fn a_report_the_daemon_cannot_take_yet_is_handed_to_the_drain() {
    let mut rig = rig(report_busy);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let reports = rig
        .calls()
        .iter()
        .filter(|call| call.verb == Verb::Report)
        .count();
    assert_eq!(reports, 1, "the lease posts once; the drain does the rest");
    assert_eq!(
        ReportSpool::new(&rig.home).pending().await.unwrap().len(),
        1
    );
    assert!(drain_rung(&rig).await);
}

#[tokio::test(start_paused = true)]
async fn a_report_refused_for_the_token_stops_the_runner_and_stays_spooled() {
    let rig = rig(report_unauthorized);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert!(rig.lessee.halt.token_refused());
    assert_eq!(
        ReportSpool::new(&rig.home).pending().await.unwrap().len(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_memory_push_still_reports() {
    let refused: Special = |call| {
        (call.verb == Verb::Capture).then(|| Answer::Fail(error::refused(Verb::Capture, 409, None)))
    };
    let mut rig = rig(refused);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert!(position(&calls, Verb::Capture).unwrap() < position(&calls, Verb::Report).unwrap());
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
}

/// A directory occupies this lease's report path, so its durable write fails.
fn unspoolable(special: Special) -> Rig {
    let rig = rig(special);
    fs::create_dir(rig.home.spool().join(format!("{LEASE_ID}.json"))).unwrap();
    rig
}

#[tokio::test(start_paused = true)]
async fn a_report_with_nowhere_to_wait_is_posted_directly_and_leasing_stops() {
    let mut rig = unspoolable(|_call| None);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert_eq!(
        reported(&rig.calls())[OUTCOME],
        PROCESSED,
        "posted all the same"
    );
    assert!(
        rig.lessee.halt.leasing().is_cancelled(),
        "no new lease without a spool"
    );
    assert!(
        !rig.lessee.halt.serving().is_cancelled(),
        "the heartbeat goes on"
    );
}

#[tokio::test(start_paused = true)]
async fn an_unspooled_report_the_daemon_will_not_take_is_logged_or_stops_the_runner() {
    let busy = unspoolable(report_busy);
    busy.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    assert!(busy.lessee.halt.leasing().is_cancelled());
    assert!(!busy.lessee.halt.token_refused());

    // Refused for the token, it is never recorded either: the runner stops,
    // and the sandbox its lease parked is destroyed.
    let refused = unspoolable(report_unauthorized);
    ends_what_it_parked(&refused).await;
    assert!(refused.lessee.halt.token_refused());
}

fn report_superseded(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Report).then(|| {
        Answer::Fail(error::refused(
            Verb::Report,
            409,
            Some(RUN_STALE_FENCING_TOKEN),
        ))
    })
}

/// The event every release of a held sandbox is logged under.
const RELEASED: &str = "sandbox_hold_released";

/// A report the daemon cannot read and never will.
fn report_rejected(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Report).then(|| Answer::Fail(error::refused(Verb::Report, 400, None)))
}

/// Runs a lease on `rig` with room to hold its sandbox, and proves the
/// daemon's answer to its report ended that hold as superseded: the daemon
/// never recorded the run the sandbox carries, so it serves no next lease.
/// Hands back what the run logged.
async fn ends_what_it_parked(rig: &Rig) -> Capture {
    let capture = Capture::install();
    rig.lessee.holds.resize(2);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    let held = rig.lessee.holds.fleets().await;
    rig.lessee.holds.shutdown().await;

    assert!(held.is_empty(), "{held:?}");
    let released = capture.only(RELEASED);
    assert_eq!(released.field("fleet_id"), Some(FLEET_ID));
    let superseded = Release::Superseded.outcome().as_str();
    assert_eq!(released.field("reason"), Some(superseded));
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
    capture
}

/// How many times the run logged an unspooled report as lost.
fn logged_lost(capture: &Capture) -> usize {
    let events = capture.events();
    events
        .iter()
        .filter(|event| event.field("event") == Some(super::EVENT_UNSPOOLED_LOST))
        .count()
}

/// A report with nowhere to wait, which the daemon answers as settled without
/// it, destroys the sandbox its lease parked.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_superseded_report_destroys_what_it_parked() {
    let capture = ends_what_it_parked(&unspoolable(report_superseded)).await;
    assert_eq!(
        logged_lost(&capture),
        0,
        "a report the daemon settled is not lost"
    );
}

/// A spooled report the daemon will never take is set aside, and the sandbox
/// its lease parked is destroyed.
#[tokio::test(start_paused = true)]
async fn test_a_rejected_report_destroys_what_it_parked() {
    ends_what_it_parked(&rig(report_rejected)).await;
}

/// A report with nowhere to wait that the daemon will never take destroys the
/// sandbox its lease parked.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_rejected_report_destroys_what_it_parked() {
    ends_what_it_parked(&unspoolable(report_rejected)).await;
}

/// A report with nowhere to wait that never reached the daemon is lost, and
/// the sandbox its lease parked is destroyed.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_report_lost_destroys_what_it_parked() {
    let capture = ends_what_it_parked(&unspoolable(report_busy)).await;
    assert_eq!(
        logged_lost(&capture),
        1,
        "a report that never arrived is logged lost"
    );
}

/// A lease carrying an empty memory window, as a daemon at this release
/// issues every lease, an empty window included.
fn window_lease() -> afd_wire::lease::LeasePayload<'static> {
    let mut payload = lease(LEASE_ID, FLEET_ID);
    payload.memory = Some(afd_wire::memory::MemoryHydrateResponse {
        memory: Vec::new(),
        shared: Vec::new(),
        publish: false,
    });
    payload
}

/// The memory and report calls a lease made, in order.
fn settle_verbs(calls: &[Call]) -> Vec<Verb> {
    calls
        .iter()
        .map(|call| call.verb)
        .filter(|verb| matches!(verb, Verb::Hydrate | Verb::Capture | Verb::Report))
        .collect()
}

/// The fake agent's one memory entry, as the wire spells it.
fn the_runs_memory() -> serde_json::Value {
    serde_json::json!([{ "key": "k", "content": "v", "category": "core" }])
}

/// What one report body carried of the one-post fields.
fn one_post_fields(body: &serde_json::Value) -> (Option<serde_json::Value>, bool) {
    let segments = body["telemetry"].get("segments");
    let timed = segments.is_some_and(|segments| segments["settle_ms"].is_u64());
    (body.get("memory").cloned(), timed)
}

/// A lease that carried its window reads no memory and settles in one post
/// carrying the run's memory and its segments.
#[tokio::test(start_paused = true)]
async fn test_settle_is_one_post_on_a_window_lease() {
    let mut rig = rig(|_call| None);

    rig.run(&window_lease()).await.unwrap();

    let calls = rig.calls();
    assert_eq!(
        (settle_verbs(&calls), one_post_fields(&reported(&calls))),
        (vec![Verb::Report], (Some(the_runs_memory()), true))
    );
}

/// A lease without a window, from an older daemon, reads memory, pushes it,
/// and sends a report without the fields that daemon would refuse.
#[tokio::test(start_paused = true)]
async fn test_settle_without_a_window_posts_as_before() {
    let mut rig = rig(|_call| None);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(
        (settle_verbs(&calls), one_post_fields(&reported(&calls))),
        (
            vec![Verb::Hydrate, Verb::Capture, Verb::Report],
            (None, false)
        )
    );
}

/// A one-post report an older replica refuses for its shape falls back once:
/// the memory through the push, the report again without the new fields, and
/// the refused copy, which holds the memory in clear, removed from the spool.
#[tokio::test(start_paused = true)]
async fn test_shape_refused_report_falls_back_once() {
    let capture = Capture::install();
    let reports = std::sync::atomic::AtomicUsize::new(0);
    let mut rig = Rig::new(
        daemon(move |call| {
            (call.verb == Verb::Report && reports.fetch_add(1, Ordering::SeqCst) == 0).then(|| {
                let shape = Some(afd_core::error_code::INVALID_REQUEST);
                Answer::Fail(error::refused(Verb::Report, 400, shape))
            })
        }),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&window_lease()).await.unwrap();

    let calls = rig.calls();
    let first = calls
        .iter()
        .find(|call| call.verb == Verb::Report)
        .and_then(|call| call.body.as_ref())
        .map(|body| serde_json::from_slice::<serde_json::Value>(body).unwrap())
        .unwrap();
    // Reports only: the authoring spool keeps its own directory beside them.
    let leftovers: Vec<_> = fs::read_dir(rig.home.spool())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    assert_eq!(
        (
            settle_verbs(&calls),
            one_post_fields(&first),
            one_post_fields(&reported(&calls)),
            leftovers,
        ),
        (
            vec![Verb::Report, Verb::Capture, Verb::Report],
            (Some(the_runs_memory()), true),
            (None, false),
            Vec::<std::path::PathBuf>::new(),
        )
    );
    let fell_back = capture.only("report_shape_fell_back");
    assert_eq!(
        (
            fell_back.field("lease_id"),
            fell_back.field("cause_error_code")
        ),
        (Some(LEASE_ID), Some("UZ-REQ-001"))
    );
}
