//! Callback attempts, durable fires, and missing snapshots remain distinct.
use af_observability::metrics::label::scheduler::{FireOutcome, ScheduleState, SyncOutcome};
use af_observability::producers::{http, scheduler};
use af_observability::test_util::Capture;
use std::time::Duration;
#[test]
fn should_export_schedule_decisions_and_matched_http_requests() {
    let captured = Capture::install();
    for answer in af_observability::metrics::label::fleet::ReportAnswer::ALL {
        af_observability::producers::fleet::report_answered(*answer);
        assert_eq!(
            captured.sum(
                "agentsfleet_runner_report_answers_total",
                &[("outcome", answer.as_str())]
            ),
            1
        );
    }
    for outcome in FireOutcome::ALL {
        scheduler::callback(*outcome, Duration::from_millis(5));
        assert_eq!(
            captured.sum(
                "agentsfleet_schedule_callbacks_total",
                &[("outcome", outcome.as_str())]
            ),
            1
        );
    }
    scheduler::fired(false);
    scheduler::fired(true);
    for outcome in ["accepted", "replayed"] {
        assert_eq!(
            captured.sum("agentsfleet_schedule_fires_total", &[("outcome", outcome)]),
            1
        );
    }
    {
        let _cancelled = scheduler::Callback::begin();
    }
    let mut completed = scheduler::Callback::begin();
    completed.answered(FireOutcome::Accepted);
    drop(completed);
    assert_eq!(
        captured.sum(
            "agentsfleet_schedule_callbacks_total",
            &[("outcome", "accepted")]
        ),
        2
    );
    for outcome in SyncOutcome::ALL {
        scheduler::synced(*outcome);
    }
    let at = jiff::Timestamp::constant(1_700_000_000, 0);
    scheduler::dispatched(None, at);
    scheduler::dispatched(Some(at), at);
    scheduler::dispatched(Some(at), jiff::Timestamp::constant(1_699_999_999, 0));
    assert_eq!(
        captured.histogram_count("agentsfleet_schedule_dispatch_lag_seconds", &[]),
        1
    );
    assert_eq!(
        captured.histogram_sum("agentsfleet_schedule_dispatch_lag_seconds", &[]),
        0.0
    );
    scheduler::counts(Some([0, 1, 2, 3, 4, 5, 6]));
    for (value, state) in ScheduleState::ALL.iter().enumerate() {
        assert_eq!(
            captured.gauge("agentsfleet_schedules", &[("state", state.as_str())]),
            Some(value as u64)
        );
    }
    scheduler::counts(None);
    assert_eq!(
        captured.gauge("agentsfleet_schedules", &[("state", "active")]),
        None
    );
    for (method, status) in [
        ("GET", 100),
        ("POST", 200),
        ("PUT", 302),
        ("PATCH", 400),
        ("DELETE", 500),
        ("OPTIONS", 999),
        ("HEAD", 204),
        ("EXTENSION", 200),
    ] {
        let mut request = http::Request::begin("/v1/fleets/{fleet_id}", method);
        request.response(status);
    }
    {
        let _cancelled = http::Request::begin("/v1/fleets/{fleet_id}", "GET");
    }
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_api_request_duration_seconds",
            &[
                ("http.route", "/v1/fleets/{fleet_id}"),
                ("http.request.method", "GET"),
                ("status_class", "cancelled")
            ]
        ),
        1
    );
}
