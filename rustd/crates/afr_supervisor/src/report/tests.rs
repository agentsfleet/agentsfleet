use std::time::Duration;

use afd_wire::report::{FailureClass, Outcome};
use afr_agent::{ExecutionResult, Failure, ResultOutcome};

use afr_agent::Meter;
use afr_providers::Usage;

use super::{Ending, narrow, report};
use crate::test_support::{FENCING, FLEET_ID, LEASE_ID, answer, lease};

/// A meter that counted `input` fresh, `cached_input` cached and `output`.
fn spent(input: u64, cached_input: u64, output: u64) -> Meter {
    let meter = Meter::default();
    meter.add(Usage {
        input,
        cached_input,
        cache_written: 0,
        output,
    });
    meter
}

#[test]
fn a_completed_run_reports_its_answer_tokens_and_timings() {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Ran {
        output: answer(),
        first_chunk: Some(Duration::from_millis(120)),
    };

    let report = report(
        &lease,
        &ending,
        &spent(3, 1, 4),
        Duration::from_secs(2),
        None,
    );

    assert_eq!(report.outcome, Outcome::Processed);
    assert_eq!(report.failure_reason, None);
    assert_eq!(report.fencing_token, FENCING);
    assert_eq!(report.response_text, "done");
    assert_eq!(report.checkpoint.last_response, "done");
    assert_eq!(report.checkpoint.last_event_id, lease.event.event_id);
    assert_eq!(
        (
            report.tokens,
            report.input_tokens,
            report.cached_input_tokens,
            report.output_tokens
        ),
        (8, 3, 1, 4),
        "the whole prompt and the completion, then the three counts apart"
    );
    assert_eq!(report.telemetry.time_to_first_token_ms, 120);
    assert_eq!(report.telemetry.wall_ms, 2_000);
}

#[test]
fn a_fleet_failure_inside_a_finished_run_reports_its_class() {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let mut output = answer();
    output.result = ExecutionResult {
        outcome: ResultOutcome::Failed(Failure {
            class: Some(FailureClass::PolicyDeny),
            detail: "denied".into(),
        }),
        ..output.result
    };
    let ending = Ending::Ran {
        output,
        first_chunk: None,
    };

    let report = report(&lease, &ending, &Meter::default(), Duration::ZERO, None);

    assert_eq!(report.outcome, Outcome::FleetError);
    assert_eq!(report.failure_reason, Some(FailureClass::PolicyDeny));
    assert_eq!(report.failure_detail, "denied");
    assert_eq!(report.telemetry.time_to_first_token_ms, 0);
}

#[test]
fn a_run_that_never_finished_still_reports_what_it_spent() {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Failed {
        class: FailureClass::RunnerCrash,
        detail: "the engine broke mid-run",
    };

    let report = report(
        &lease,
        &ending,
        &spent(7, 2, 3),
        Duration::from_millis(5),
        None,
    );

    assert_eq!(report.outcome, Outcome::FleetError);
    assert_eq!(report.failure_reason, Some(FailureClass::RunnerCrash));
    assert_eq!(report.failure_detail, "the engine broke mid-run");
    assert_eq!(report.response_text, "");
    assert_eq!(
        (
            report.tokens,
            report.input_tokens,
            report.cached_input_tokens,
            report.output_tokens
        ),
        (12, 7, 2, 3),
        "the turns the meter counted before the break are billed"
    );
}

#[test]
fn a_run_that_never_started_reports_zero_usage() {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Failed {
        class: FailureClass::StartupPosture,
        detail: "no sandbox",
    };

    let report = report(
        &lease,
        &ending,
        &Meter::default(),
        Duration::from_millis(5),
        None,
    );

    assert_eq!(report.outcome, Outcome::FleetError);
    assert_eq!(report.failure_reason, Some(FailureClass::StartupPosture));
    assert_eq!(report.failure_detail, "no sandbox");
    assert_eq!((report.tokens, report.response_text.as_ref()), (0, ""));
}

#[test]
fn a_count_past_the_wire_width_saturates() {
    assert_eq!(narrow(u64::MAX), u32::MAX);
    assert_eq!(narrow(9), 9);
}

#[test]
fn a_finished_run_bills_its_result_not_the_meter() {
    let lease = lease(LEASE_ID, FLEET_ID, None);
    let ending = Ending::Ran {
        output: answer(),
        first_chunk: None,
    };

    let report = report(&lease, &ending, &spent(100, 50, 25), Duration::ZERO, None);

    assert_eq!(
        (
            report.tokens,
            report.input_tokens,
            report.cached_input_tokens,
            report.output_tokens
        ),
        (8, 3, 1, 4),
        "what the engine handed back is what is billed; the meter is the fallback"
    );
}
