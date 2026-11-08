//! `POST /v1/runners/me/reports` — the terminal result of one run.
//!
//! Thin for the reason [`super::lease`] is: the fence, the flip, the money and
//! the five finalize writes are all `afd_fleet::lease::Plane`'s, and what is
//! left here is which identity is asking and what a refusal looks like on the
//! wire.
//!
//! # The body IS read, and strictly
//!
//! Unlike the lease poll, this request carries the only copy of what happened —
//! the verdict, the token counts a tenant is charged for, and the cursor the
//! session resumes from. `ReportRequest` is `deny_unknown_fields`, so a runner
//! from a newer build sending a field this daemon does not know is refused
//! rather than silently charged against a partial reading.
//!
//! # Every refusal here is terminal for the run
//!
//! A stale fence, a lease that is not this runner's — both mean the result is
//! not wanted and retrying cannot change that. The runner discards and moves
//! on. Only a datastore failure is worth a retry, and that arrives as a 503
//! through the transport class the code carries, which is what the runner
//! already backs off on.
//!
//! # Why the run's telemetry is described HERE and not where it settled
//!
//! A settled report is the one moment this daemon knows what a whole run cost
//! and how long it took, and two records are owed for it: a product event and
//! a delivery span. Neither is written on the money path. `afd_fleet` answers
//! with the facts — the charge, and the identity the lease row resolved — and
//! this handler decides what to say about them, so a lease plane stays usable
//! by a process with no exporter and no analytics project configured.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use af_observability::metrics::label::cost::ErrorType;
use af_observability::producers::cost::{self, Spend};
use af_observability::producers::fleet::runner;
use af_observability::{Delivery, Segments, Telemetry};
use af_wire::report::{Outcome, ReportRequest, ReportResponse, RunSegments};
use afd_fleet::lease::report::Reconciled;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use super::usage::{self, DETAIL_REPORT_MALFORMED};
use crate::auth::RunnerIdentity;
use crate::handler::{malformed, refuse};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed report is logged under.
const EVENT: &str = "runner_report_failed";

/// What [`handle`] documents, its caps spelled from their constants.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = const_format::concatcp!(
    "Saves the final result of a work assignment. Replaced assignments are refused ",
    "without saving anything. Retry the same report safely using its lease ",
    "identifier; no `Idempotency-Key` is accepted. Repeats return the saved result ",
    "without another charge or run count. Invalid bodies return `UZ-REQ-001` with the ",
    "rejected field or limit: retired cumulative totals, more than ",
    af_wire::usage::LINES_MAX,
    " `requests`, or `seq` above ",
    af_wire::usage::RUNNER_SEQ_MAX,
    ".",
);

/// Records one terminal execution result.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUNNER_REPORTS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_report",
    summary = "Report the result of one run",
    description = DESCRIPTION,
    request_body = ReportRequest<'_>,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ReportResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Response {
    // Borrowed out of `body`, which outlives the call. The response text and
    // the checkpoint are the two largest values on this path and both are
    // written straight into columns, so owning them here would copy a run's
    // whole output for nothing.
    let request = match afd_http::handler::read_body::<ReportRequest<'_>>(&body) {
        Ok(request) => request,
        // A runner from before usage moved to request lines is told which
        // field it sent; any other unreadable body gets the general sentence.
        Err(failure) => {
            return malformed(usage::retired_total(&failure).unwrap_or(DETAIL_REPORT_MALFORMED));
        }
    };
    // The request lines are money: one past the batch cap, or numbered in the
    // daemon's own range, is refused rather than partly priced, naming the
    // bound it broke.
    if let Err(report) = garde::Validate::validate(&request.requests) {
        return malformed(usage::REPORT_LINES.pick(&report));
    }

    let received = services.now();
    match services
        .leases()
        .report(runner.id(), &request, received)
        .await
    {
        // The drained amount is answered by the plane and deliberately not put
        // on the wire: a runner has no use for it, and the reply the stock
        // runner parses is `{"ok":true}`. It exists so M181 §5 can attach the
        // credit instrument without reopening the money path.
        // A REPEATED report describes a run that was already described. The
        // plane answered success so the runner stops retrying and its result
        // is not thrown away, and everything below says something ABOUT the
        // run — the completion funnel, the cost meters, the delivery span —
        // so firing it again would count one run as two. The charge is already
        // zero on this arm; the counters are not, and they are the reason this
        // is a branch rather than arithmetic.
        Ok(settled) if settled.repeated => {
            af_observability::producers::fleet::report_answered(
                af_observability::metrics::label::fleet::ReportAnswer::Repeated,
            );
            Json(ReportResponse { ok: true }).into_response()
        }
        Ok(settled) => {
            af_observability::producers::fleet::report_answered(
                af_observability::metrics::label::fleet::ReportAnswer::Settled,
            );
            // Attributed to the WORKSPACE, as the daemon this ports attributes
            // it: a run has no person behind it — a cron, a webhook and a steer
            // all end here — and the workspace is the unit the funnels count.
            services.analytics().report(&Telemetry::FleetCompleted {
                actor: settled.workspace_id.as_str().to_owned(),
                workspace_id: settled.workspace_id.as_str().to_owned(),
                fleet_id: settled.fleet_id.as_str().to_owned(),
                event_id: request.event_id.as_ref().to_owned(),
                tokens: request.tokens,
                wall_ms: request.telemetry.wall_ms,
                exit_status: request.outcome.as_str().to_owned(),
                time_to_first_token_ms: u64::from(request.telemetry.time_to_first_token_ms),
            });
            // Read here rather than reused from `services.now()`: that instant
            // was taken BEFORE the settle, and the span's end is the moment the
            // run's record actually closed.
            #[expect(
                clippy::disallowed_methods,
                reason = "a span ends at a SystemTime at the host's full precision; af_core::clock::now truncates to milliseconds"
            )]
            let ended = settled
                .completed_at
                .map_or_else(SystemTime::now, SystemTime::from);
            let complete = settled
                .completed_at
                .map(|completed| af_core::clock::elapsed(received, completed));
            delivery_of(&settled, &request, complete).record(ended);
            meter(&settled, &request, runner.id().as_str());
            Json(ReportResponse { ok: true }).into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}

/// Records what the finished run cost, and what its runner did.
///
/// Everything here is derived from facts the settle already produced, so a
/// deployment exporting nothing pays one branch. The per-runner counters go
/// through the RAW identifier deliberately: the slot table decides what it is
/// attributed to, and a caller that chose the label would make the cardinality
/// ceiling a suggestion.
fn meter(settled: &Reconciled, request: &ReportRequest<'_>, runner_id: &str) {
    match request.failure_reason {
        // A failure the runner classified, and the classification is the label.
        reason @ Some(_) => runner::failed(runner_id, reason),
        // No class reported. `Outcome` still decides which it was: a fleet
        // error with no class is counted under the unmodelled bucket rather
        // than as a clean run, because the totals have to agree.
        None => match request.outcome {
            Outcome::Processed => runner::processed(runner_id),
            Outcome::FleetError => runner::failed(runner_id, None),
        },
    }

    cost::invocation(&spend_of(settled, request));

    // Spend is recorded where the biller settles, once per settlement, so
    // renewals, this report and the sweeper's retries all count there and
    // nowhere else.
}

/// The finished run, as the cost families record it.
///
/// Input is the whole prompt for the reason [`delivery_of`] gives; the cache
/// reads are recorded again as their own subset family.
fn spend_of<'a>(settled: &'a Reconciled, request: &ReportRequest<'_>) -> Spend<'a> {
    Spend {
        model: &settled.model,
        posture: &settled.posture,
        input_tokens: count(settled.tokens.prompt()),
        cached_input_tokens: count(settled.tokens.cached_input_tokens),
        output_tokens: count(settled.tokens.output_tokens),
        wall: reported_wall(request),
        error: (request.outcome == Outcome::FleetError).then_some(ErrorType::FleetError),
    }
}

/// The finished run, as the span that describes it.
///
/// Borrowed from what the settle produced: the lease row's identity, which
/// only the plane could resolve, and the tokens its usage lines record.
///
/// Input is the whole prompt, cache reads and writes included, because the
/// standard's `gen_ai.usage.input_tokens` counts every cached class: the lines
/// store the three apart, so leaving any out would undercount the run.
fn delivery_of<'a>(
    settled: &'a Reconciled,
    request: &'a ReportRequest<'a>,
    complete: Option<Duration>,
) -> Delivery<'a> {
    Delivery {
        tenant_id: settled.tenant_id.as_str(),
        workspace_id: settled.workspace_id.as_str(),
        fleet_id: settled.fleet_id.as_str(),
        event_id: &settled.event_id,
        posture: &settled.posture,
        provider: &settled.provider,
        model: &settled.model,
        input_tokens: count(settled.tokens.prompt()),
        output_tokens: count(settled.tokens.output_tokens),
        wall: reported_wall(request),
        opened_at: Some(SystemTime::from(settled.event_created_at)),
        segments: segments_of(settled, request.telemetry.segments, complete),
    }
}

/// The round trip's segments: the queue wait and `complete` (the report's
/// receipt to its completion publish) measured on this daemon's clock, the
/// rest as the runner reported them. The span caps every one.
fn segments_of(
    settled: &Reconciled,
    runner: Option<RunSegments>,
    complete: Option<Duration>,
) -> Segments {
    let runner = runner.unwrap_or_default();
    let reported = |ms: Option<u64>| ms.map(Duration::from_millis);
    Segments {
        queue_wait: Some(af_core::clock::elapsed(
            settled.event_created_at,
            settled.lease_created_at,
        )),
        memory: reported(runner.memory_ms),
        model: reported(runner.model_ms),
        tools: reported(runner.tools_ms),
        sandbox_wait: reported(runner.sandbox_wait_ms),
        settle: reported(runner.settle_ms),
        complete,
    }
}

/// A token sum as the metrics take it; a sum is never negative.
fn count(sum: i64) -> u64 {
    u64::try_from(sum).unwrap_or(0)
}

/// How long the runner says the run took, bounded.
///
/// The number is the runner's and nothing validates it upstream, so it is
/// capped by the same bound the delivery span uses. Uncapped it reaches the
/// duration histogram's SUM — the value every mean and latency panel divides
/// by its count — where one absurd report is not an outlier a percentile
/// absorbs but a total nothing recovers from until the window rolls.
fn reported_wall(request: &ReportRequest<'_>) -> Duration {
    Duration::from_millis(request.telemetry.wall_ms).min(af_observability::MAX_RUN)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use af_core::id::Uuid7;
    use afd_fleet::lease::report::{Reconciled, RunTokens};
    use std::time::{Duration, SystemTime};

    use af_observability::Segments;
    use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry, RunSegments};

    use super::{delivery_of, spend_of};

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
            tenant_id: id,
            event_id: "event-fixture".to_owned(),
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

        let spend = spend_of(&settled, &request);
        assert_eq!(spend.input_tokens, PROMPT, "reads and writes are input");
        assert_eq!(spend.cached_input_tokens, 400, "the read subset alone");
        assert_eq!(spend.output_tokens, 90);

        let delivery = delivery_of(&settled, &request, None);
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

        let delivery = delivery_of(&settled, &request, complete);

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
        let delivery = delivery_of(&settled, &request, None);
        assert_eq!(
            delivery.segments,
            Segments {
                queue_wait: Some(Duration::from_millis(QUEUE_WAIT_MS)),
                ..Segments::default()
            }
        );
        Ok(())
    }
}
