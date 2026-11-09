//! Completion observations preserve token sums, identity and measured segments.

use std::borrow::Cow;

use af_core::id::Uuid7;
use crate::lease::report::{Reconciled, RunTokens};
use std::time::{Duration, SystemTime};

use af_observability::Segments;
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry, RunSegments};

use super::{UsageCounts, delivery_of, spend_of};

const UNCACHED_INPUT_TOKENS: i64 = 1_000;

/// One run's usage lines, summed: every prompt class non-zero and distinct,
/// so a total that dropped or doubled any one of them cannot match.
const TOKENS: RunTokens = RunTokens {
    input_tokens: UNCACHED_INPUT_TOKENS,
    cached_input_tokens: 400,
    cache_write_tokens: 250,
    output_tokens: 90,
};

/// The whole prompt [`TOKENS`] carries.
const PROMPT: u64 = 1_650;

/// A first report of a settled run whose lines summed to `tokens`.
fn settled(tokens: RunTokens) -> Result<Reconciled, &'static str> {
    let id = Uuid7::parse("01924f4e-0000-7000-8000-00000000fee7")
        .map_err(|_malformed| "a fixture identifier is well formed")?;
    Ok(Reconciled {
        charged: afd_billing::Nanos::ZERO,
        fleet_id: id.clone(),
        workspace_id: id.clone(),
        tenant_id: id.clone(),
        event_id: id,
        posture: "platform".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5".to_owned(),
        repeated: false,
        tokens,
        event_created_at: af_core::clock::instant_at(after(0)),
        lease_created_at: af_core::clock::instant_at(after(QUEUE_WAIT_MS)),
        completed_at: Some(af_core::clock::instant_at(after(COMPLETED_MS))),
    })
}

/// When the fixture event was created.
const CREATED_AT_SECONDS: u64 = 1_767_225_600;
/// Event creation to lease issue, in the fixture.
const QUEUE_WAIT_MS: u64 = 40;
/// Event creation to the completion publish, in the fixture.
const COMPLETED_MS: u64 = 900;

/// The fixture's moment `offset_ms` after the event was created.
fn after(offset_ms: u64) -> SystemTime {
    created() + Duration::from_millis(offset_ms)
}

/// When the fixture event was created, as the span's start reads it.
fn created() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(CREATED_AT_SECONDS)
}

/// A processed report; the totals under test come from the lines, not here.
fn request() -> ReportRequest<'static> {
    ReportRequest {
        folded_through_sequence: None,
        task: None,
        lease_id: Cow::Borrowed("01924f4e-0000-7000-8000-00000000fee7"),
        event_id: Cow::Borrowed("event-fixture"),
        fencing_token: 1,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed("done"),
        tokens: 0,
        requests: af_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 1_250,
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed("event-fixture"),
            last_response: Cow::Borrowed("done"),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}

/// A run whose lines wrote the cache counts the writes as input on the
/// cost family and the delivery span alike; the reads stay their own
/// subset and output is untouched.
#[test]
fn test_cache_writes_count_as_input_tokens() -> Result<(), &'static str> {
    let settled = settled(TOKENS)?;
    let request = request();

    let usage = UsageCounts::try_from(&settled.tokens).map_err(|_| "saved usage is nonnegative")?;
    let spend = spend_of(&settled, &request, &usage);
    assert_eq!(spend.input_tokens, PROMPT, "reads and writes are input");
    assert_eq!(spend.cached_input_tokens, 400, "the read subset alone");
    assert_eq!(spend.output_tokens, 90);

    let delivery = delivery_of(&settled, &request, None, &usage);
    assert_eq!(
        (delivery.input_tokens, delivery.output_tokens),
        (PROMPT, 90),
        "the span agrees with the meter"
    );
    Ok(())
}

/// The span starts at the event's creation and carries the daemon's two
/// segments beside the runner's, each where it was measured.
#[test]
fn test_span_joins_daemon_and_runner_segments() -> Result<(), &'static str> {
    let settled = settled(TOKENS)?;
    let mut request = request();
    request.telemetry.segments = Some(RunSegments {
        memory_ms: None,
        model_ms: Some(500),
        tools_ms: Some(0),
        sandbox_wait_ms: None,
        settle_ms: Some(30),
    });
    let complete = Some(Duration::from_millis(120));

    let usage = UsageCounts::try_from(&settled.tokens).map_err(|_| "saved usage is nonnegative")?;
    let delivery = delivery_of(&settled, &request, complete, &usage);

    assert_eq!(
        (delivery.opened_at, delivery.segments),
        (
            Some(created()),
            Segments {
                queue_wait: Some(Duration::from_millis(QUEUE_WAIT_MS)),
                memory: None,
                model: Some(Duration::from_millis(500)),
                tools: Some(Duration::ZERO),
                sandbox_wait: None,
                settle: Some(Duration::from_millis(30)),
                complete,
            },
        )
    );
    Ok(())
}

/// A runner from before the segments rode the report leaves only what the
/// daemon measured itself.
#[test]
fn test_span_without_runner_segments_keeps_the_daemons() -> Result<(), &'static str> {
    let settled = settled(TOKENS)?;
    let request = request();
    let usage = UsageCounts::try_from(&settled.tokens).map_err(|_| "saved usage is nonnegative")?;
    let delivery = delivery_of(&settled, &request, None, &usage);
    assert_eq!(
        delivery.segments,
        Segments {
            queue_wait: Some(Duration::from_millis(QUEUE_WAIT_MS)),
            ..Segments::default()
        }
    );
    Ok(())
}

/// Corrupt saved usage must never appear as a successful zero measurement.
#[test]
fn negative_saved_usage_is_an_error() {
    let tokens = RunTokens { input_tokens: -1, cached_input_tokens: 0, cache_write_tokens: 0, output_tokens: 0 };
    assert!(UsageCounts::try_from(&tokens).is_err());
}
