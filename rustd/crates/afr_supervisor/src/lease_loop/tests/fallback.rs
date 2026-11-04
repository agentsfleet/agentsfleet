//! The settle's fall-back when an older replica refuses the one-post report's
//! shape, mid-deploy: the memory through the push, the report once more
//! without the fields that replica does not read.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use afd_core::error_code::{INTERNAL_OPERATION_FAILED, INVALID_REQUEST};
use afd_core::test_util::trace::Capture;

use super::{one_post_fields, settle_verbs, the_runs_memory, window_lease, without_spool};
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{Answer, Behaviour, FakeAgent, FakeEngine, LEASE_ID, Rig, daemon};

/// A daemon that refuses the first report post's shape, as an older replica
/// does, runs `then` on every later one, and otherwise answers as the fixture
/// daemon does.
fn refuses_first_shape(
    then: impl Fn() + Send + Sync + 'static,
) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let reports = AtomicUsize::new(0);
    daemon(move |call| {
        if call.verb != Verb::Report {
            return None;
        }
        if reports.fetch_add(1, Ordering::SeqCst) == 0 {
            let shape = Some(INVALID_REQUEST);
            return Some(Answer::Fail(error::refused(Verb::Report, 400, shape)));
        }
        then();
        None
    })
}

fn rig(daemon: impl Fn(&Call) -> Answer + Send + Sync + 'static, agent: Behaviour) -> Rig {
    Rig::new(daemon, FakeEngine::default(), FakeAgent::new(agent))
}

/// The one-post fields of each report body the lease posted, in order.
fn reports_carried(calls: &[Call]) -> Vec<(Option<serde_json::Value>, bool)> {
    calls
        .iter()
        .filter(|call| call.verb == Verb::Report)
        .filter_map(|call| call.body.as_ref())
        .map(|body| one_post_fields(&serde_json::from_slice(body).unwrap()))
        .collect()
}

/// Every file left in the spool. Reports only: the authoring spool keeps its
/// own directory beside them.
fn spooled_files(rig: &Rig) -> Vec<PathBuf> {
    fs::read_dir(rig.home.spool())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect()
}

/// A one-post report an older replica refuses for its shape falls back once:
/// the memory through the push, the report again without the new fields, and
/// the refused copy, which holds the memory in clear, removed from the spool.
#[tokio::test(start_paused = true)]
async fn test_shape_refused_report_falls_back_once() {
    let capture = Capture::install();
    let mut rig = rig(refuses_first_shape(|| {}), Behaviour::Answer);

    rig.run(&window_lease()).await.unwrap();

    let calls = rig.calls();
    assert_eq!(
        (
            settle_verbs(&calls),
            reports_carried(&calls),
            spooled_files(&rig)
        ),
        (
            vec![Verb::Report, Verb::Capture, Verb::Report],
            vec![(Some(the_runs_memory()), true), (None, false)],
            Vec::new(),
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

/// A run that failed hands on no memory, so its refused one-post report,
/// segments alone, falls back to the plain report with no memory push.
#[tokio::test(start_paused = true)]
async fn test_a_failed_run_falls_back_without_a_memory_push() {
    let mut rig = rig(refuses_first_shape(|| {}), Behaviour::Break);

    rig.run(&window_lease()).await.unwrap();

    let calls = rig.calls();
    assert_eq!(
        (settle_verbs(&calls), reports_carried(&calls)),
        (
            vec![Verb::Report, Verb::Report],
            vec![(None, true), (None, false)],
        )
    );
}

/// A one-post report with nowhere to wait, refused for its shape as it goes
/// out directly, falls back the same way: memory pushed, then the plain report.
#[tokio::test(start_paused = true)]
async fn test_an_unspooled_shape_refused_report_falls_back() {
    let mut rig = without_spool(rig(refuses_first_shape(|| {}), Behaviour::Answer));

    rig.run(&window_lease()).await.unwrap();

    let calls = rig.calls();
    assert_eq!(
        (settle_verbs(&calls), reports_carried(&calls)),
        (
            vec![Verb::Report, Verb::Capture, Verb::Report],
            vec![(Some(the_runs_memory()), true), (None, false)],
        )
    );
}

/// A refused copy the spool cannot remove after the fall-back is logged and
/// left for the boot sweep; the plain report still settles the lease.
#[tokio::test(start_paused = true)]
async fn test_a_refused_copy_that_will_not_go_is_logged_and_left() {
    let capture = Capture::install();
    let spool: Arc<OnceLock<PathBuf>> = Arc::default();
    let seen = Arc::clone(&spool);
    let mut rig = rig(
        refuses_first_shape(move || {
            // The refused copy, swapped for a directory before the spool
            // removes it as a file.
            let aside = seen
                .get()
                .unwrap()
                .join(LEASE_ID)
                .with_extension("rejected");
            fs::remove_file(&aside).unwrap();
            fs::create_dir(&aside).unwrap();
        }),
        Behaviour::Answer,
    );
    spool.set(rig.home.spool()).unwrap();

    rig.run(&window_lease()).await.unwrap();

    let kept = capture.only("report_spool_rejected_kept");
    assert_eq!(
        (
            settle_verbs(&rig.calls()),
            kept.field("error_code"),
            kept.field("lease_id"),
            spooled_files(&rig),
        ),
        (
            vec![Verb::Report, Verb::Capture, Verb::Report],
            Some(INTERNAL_OPERATION_FAILED.as_str()),
            Some(LEASE_ID),
            Vec::new(),
        )
    );
}
