//! What the shared preamble admits, and which failure it reports first.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;

use super::{admitted, cancellable_on, finish};
use crate::error::Error;
use crate::profile::{PROFILE_VARIABLE, Profile, Target};
use crate::report::{Lane, Report};

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

#[test]
fn test_an_absent_profile_is_the_rig() {
    let (profile, target) = admitted(&env_of(&[])).expect("the rig needs no variables");
    assert_eq!(profile, Profile::Rig);
    assert_eq!(target, Target::Rig);
}

#[test]
fn test_a_named_profile_is_admitted_through_its_own_checks() {
    let refused = admitted(&env_of(&[(PROFILE_VARIABLE, "prod")]))
        .expect_err("prod without its acknowledgement refuses");
    assert!(
        matches!(refused, Error::AcknowledgementMissing { .. }),
        "got {refused}"
    );
}

#[test]
fn test_the_lanes_error_is_reported_before_the_sweeps() {
    let lane_failed: Result<Report, Error> = Err(Error::TaskLost { role: "runner" });
    let sweep_failed: Result<u64, Error> = Err(Error::InstrumentPoisoned);

    let refused = finish(Lane::Lease, Profile::Rig, lane_failed, sweep_failed)
        .expect_err("two failures still refuse");

    assert!(
        matches!(refused, Error::TaskLost { .. }),
        "the measurement's failure is the one a reader needs, got {refused}"
    );
}

#[test]
fn test_a_failed_sweep_after_a_good_run_is_still_a_refusal() {
    let mut report = Report::new(Lane::Lease, Profile::Rig);
    report.fixture.created = 3;

    let refused = finish(
        Lane::Lease,
        Profile::Rig,
        Ok(report),
        Err(Error::InstrumentPoisoned),
    )
    .expect_err("a run whose fixtures may be left behind is not a clean result");

    assert!(matches!(refused, Error::InstrumentPoisoned));
}

#[test]
fn test_the_callers_sweep_adds_to_what_the_lane_swept_itself() {
    let mut report = Report::new(Lane::Outbound, Profile::Rig);
    report.fixture.created = 200;
    report.fixture.swept = 200;
    let scratch = std::env::temp_dir().join(format!("afd-bench-finish-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&scratch);
    let previous = std::env::current_dir().expect("a cwd");
    std::env::set_current_dir(&scratch).expect("scratch is enterable");

    let written = finish(Lane::Outbound, Profile::Rig, Ok(report), Ok(0));

    std::env::set_current_dir(previous).expect("cwd restored");
    let path = written.expect("a finished run writes");
    let read = Report::read(std::path::Path::new(&scratch).join(path).as_path()).expect("readable");
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        read.fixture.swept, 200,
        "a fallback that found nothing must not erase what the lane swept"
    );
}

#[tokio::test]
async fn test_cancellation_is_returned_to_the_caller_that_owns_the_sweep() {
    let cancellation = CancellationToken::new();
    let wait = cancellation.clone();
    let cleaned = Arc::new(AtomicBool::new(false));
    let lane_cleaned = Arc::clone(&cleaned);
    let lane = async move {
        wait.cancelled().await;
        lane_cleaned.store(true, Ordering::SeqCst);
        Ok(())
    };
    let interrupted = std::future::ready(Ok(()));

    let refusal = cancellable_on(cancellation.clone(), lane, interrupted)
        .await
        .expect_err("an interrupt must stop the measurement");

    assert!(matches!(refusal, Error::Cancelled));
    assert!(cancellation.is_cancelled());
    assert!(
        cleaned.load(Ordering::SeqCst),
        "the wrapper waits for lane shutdown before the caller sweeps"
    );
}

#[tokio::test]
async fn test_a_completed_lane_wins_without_waiting_for_an_interrupt() {
    let lane = std::future::ready(Ok::<_, Error>(42));
    let interrupted = std::future::pending::<std::io::Result<()>>();

    assert_eq!(
        cancellable_on(CancellationToken::new(), lane, interrupted)
            .await
            .expect("the lane completed"),
        42
    );
}
