//! A placement the assignment should have filtered is logged as the error it
//! is, under its registry code, naming the runner, the reason and the work.

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use afd_runner::capacity::{PlacementRefusal, refused_placement};

#[test]
fn a_refused_placement_is_logged_with_its_reason_and_work() -> af_core::error::Result<()> {
    let runner = Uuid7::parse("0190d5c2-7e1a-7000-8000-00000000a002")?;
    for (refusal, reason) in [
        (PlacementRefusal::ForeignTenant, "foreign_tenant"),
        (PlacementRefusal::PlatformModel, "platform_model"),
    ] {
        let capture = Capture::install();
        refused_placement(&runner, refusal, "setup");
        let logged = capture.only("runner_placement_refused");
        assert_eq!(logged.field("runner_id"), Some(runner.as_str()));
        assert_eq!(logged.field("reason"), Some(reason));
        assert_eq!(logged.field("run_type"), Some("setup"));
        assert_eq!(
            logged.field("error_code"),
            Some(af_core::error_code::RUNNER_PLACEMENT_REFUSED.as_str())
        );
    }
    Ok(())
}
