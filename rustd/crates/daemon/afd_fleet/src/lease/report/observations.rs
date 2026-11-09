//! Completion observations follow durable settlement and retain replay suppression.

use super::{Reconciled, RunTokens};
use crate::Result;
use af_observability::{
    Analytics, Delivery, Segments, Telemetry,
    metrics::label::{cost::ErrorType, fleet::ReportAnswer},
    producers::{
        cost::{self, Spend},
        fleet::{report_answered, runner},
    },
};
use af_wire::report::{Outcome, ReportRequest, RunSegments};
use jiff::Timestamp;
use std::time::{Duration, SystemTime};

/// Describe one saved result; a replay acknowledges success without counting the run again.
#[expect(
    clippy::disallowed_methods,
    reason = "delivery spans end at full SystemTime precision"
)]
pub fn observe(
    analytics: &Analytics,
    settled: &Reconciled,
    request: &ReportRequest<'_>,
    runner_id: &str,
    received: Timestamp,
) -> Result<()> {
    if settled.repeated {
        report_answered(ReportAnswer::Repeated);
        return Ok(());
    }
    let usage = UsageCounts::try_from(&settled.tokens)?;
    report_answered(ReportAnswer::Settled);
    analytics.report(&Telemetry::FleetCompleted {
        actor: settled.workspace_id.as_str().to_owned(),
        workspace_id: settled.workspace_id.as_str().to_owned(),
        fleet_id: settled.fleet_id.as_str().to_owned(),
        event_id: settled.event_id.as_str().to_owned(),
        tokens: request.tokens,
        wall_ms: request.telemetry.wall_ms,
        exit_status: request.outcome.as_str().to_owned(),
        time_to_first_token_ms: u64::from(request.telemetry.time_to_first_token_ms),
    });
    let ended = settled
        .completed_at
        .map_or_else(SystemTime::now, SystemTime::from);
    let complete = settled
        .completed_at
        .map(|completed| af_core::clock::elapsed(received, completed));
    delivery_of(settled, request, complete, &usage).record(ended);
    meter(settled, request, runner_id, &usage);
    Ok(())
}

/// Records what the finished run cost, and what its runner did.
///
/// Everything here is derived from facts the settle already produced, so a
/// deployment exporting nothing pays one branch. The per-runner counters go
/// through the RAW identifier deliberately: the slot table decides what it is
/// attributed to, and a caller that chose the label would make the cardinality
/// ceiling a suggestion.
fn meter(settled: &Reconciled, request: &ReportRequest<'_>, runner_id: &str, usage: &UsageCounts) {
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

    cost::invocation(&spend_of(settled, request, usage));

    // Spend is recorded where the biller settles, once per settlement, so
    // renewals, this report and the sweeper's retries all count there and
    // nowhere else.
}

/// The finished run, as the cost families record it.
///
/// Input is the whole prompt for the reason [`delivery_of`] gives; the cache
/// reads are recorded again as their own subset family.
fn spend_of<'a>(
    settled: &'a Reconciled,
    request: &ReportRequest<'_>,
    usage: &UsageCounts,
) -> Spend<'a> {
    Spend {
        model: &settled.model,
        posture: &settled.posture,
        input_tokens: usage.prompt,
        cached_input_tokens: usage.cached,
        output_tokens: usage.output,
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
    usage: &UsageCounts,
) -> Delivery<'a> {
    Delivery {
        tenant_id: settled.tenant_id.as_str(),
        workspace_id: settled.workspace_id.as_str(),
        fleet_id: settled.fleet_id.as_str(),
        event_id: settled.event_id.as_str(),
        posture: &settled.posture,
        provider: &settled.provider,
        model: &settled.model,
        input_tokens: usage.prompt,
        output_tokens: usage.output,
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

/// Conversion happens once before any observation, retaining malformed saved data as an error.
struct UsageCounts {
    prompt: u64,
    cached: u64,
    output: u64,
}

impl TryFrom<&RunTokens> for UsageCounts {
    type Error = crate::Error;

    fn try_from(tokens: &RunTokens) -> Result<Self> {
        Ok(Self {
            prompt: u64::try_from(tokens.prompt())?,
            cached: u64::try_from(tokens.cached_input_tokens)?,
            output: u64::try_from(tokens.output_tokens)?,
        })
    }
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
mod tests;
