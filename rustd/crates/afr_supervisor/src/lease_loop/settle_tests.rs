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

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

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

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

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

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let calls = rig.calls();
    assert!(position(&calls, Verb::Capture).unwrap() < position(&calls, Verb::Report).unwrap());
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
}

/// A rig whose spool directory is gone, so no report can be held.
fn unspoolable(special: Special) -> Rig {
    let rig = rig(special);
    fs::remove_dir(rig.home.spool()).unwrap();
    rig
}

#[tokio::test(start_paused = true)]
async fn a_report_with_nowhere_to_wait_is_posted_directly_and_leasing_stops() {
    let mut rig = unspoolable(|_call| None);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

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
    busy.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    assert!(busy.lessee.halt.leasing().is_cancelled());
    assert!(!busy.lessee.halt.token_refused());

    let refused = unspoolable(report_unauthorized);
    refused.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
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
async fn ends_what_it_parked(rig: Rig) {
    let capture = Capture::install();
    rig.lessee.holds.resize(2);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    let held = rig.lessee.holds.fleets().await;
    rig.lessee.holds.shutdown().await;

    assert!(held.is_empty(), "{held:?}");
    let released = capture.only(RELEASED);
    assert_eq!(released.field("fleet_id"), Some(FLEET_ID));
    let superseded = Release::Superseded.outcome().as_str();
    assert_eq!(released.field("reason"), Some(superseded));
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

/// A report with nowhere to wait, which the daemon answers as settled without
/// it, destroys the sandbox its lease parked.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_superseded_report_destroys_what_it_parked() {
    ends_what_it_parked(unspoolable(report_superseded)).await;
}

/// A spooled report the daemon will never take is set aside, and the sandbox
/// its lease parked is destroyed.
#[tokio::test(start_paused = true)]
async fn test_a_rejected_report_destroys_what_it_parked() {
    ends_what_it_parked(rig(report_rejected)).await;
}

/// A report with nowhere to wait that the daemon will never take destroys the
/// sandbox its lease parked.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_rejected_report_destroys_what_it_parked() {
    ends_what_it_parked(unspoolable(report_rejected)).await;
}

/// A report with nowhere to wait that never reached the daemon is lost, and
/// the sandbox its lease parked is destroyed.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_report_lost_destroys_what_it_parked() {
    ends_what_it_parked(unspoolable(report_busy)).await;
}
