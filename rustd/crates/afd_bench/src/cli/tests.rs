//! What the shared preamble admits, and which failure it reports first.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::collections::HashMap;

use super::{admitted, finish};
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
