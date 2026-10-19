//! Daemon receipts are authoritative; runner observations only advance runtime.

use std::time::Duration;

use afd_billing::RUN_NANOS_PER_SEC;
use afd_wire::authoring::{Knowledge, Meter, Usage};
use sqlx::PgConnection;

use super::guard::Attempt;
use crate::{
    Result,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    model::dispatch_status,
};

const MILLIS_PER_SECOND: i128 = 1000;
const CONTEXT: &str = "verify authoring cumulative usage";
const TOTALS: &str = include_str!("sql/totals.sql");

pub(crate) struct Verified {
    pub(crate) runtime: Duration,
    pub(crate) usage: Usage,
    pub(crate) failed_calls: i64,
}

#[derive(sqlx::FromRow)]
struct Totals {
    calls: i64,
    input: i64,
    output: i64,
    provider: i64,
    known: bool,
    failed_calls: i64,
}

pub(crate) async fn verify(
    connection: &mut PgConnection,
    attempt: &Attempt,
    reported: &garde::Valid<Meter>,
    now: jiff::Timestamp,
) -> Result<Verified> {
    let totals: Totals = sqlx::query_as(TOTALS)
        .bind(attempt.id.as_str())
        .bind(dispatch_status::KNOWN)
        .bind(dispatch_status::PENDING)
        .bind(dispatch_status::UNKNOWN)
        .bind(dispatch_status::FAILED)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    let runtime = observations(attempt, reported, &totals, now)?;
    check_cost(reported, totals.provider)?;
    let runtime_nanos = runtime_nanos(runtime)?;
    let usage = usage(
        totals.known,
        totals.input,
        totals.output,
        totals.provider,
        runtime_nanos,
    );
    Ok(Verified {
        runtime,
        failed_calls: totals.failed_calls,
        usage: usage?,
    })
}

/// The runtime charge for `runtime`, rounded up to the nano.
fn runtime_nanos(runtime: Duration) -> Result<i64> {
    i64::try_from(
        (i128::try_from(runtime.as_millis()).map_err(|_overflow| ErrorKind::Balance)?
            * i128::from(RUN_NANOS_PER_SEC)
            + MILLIS_PER_SECOND
            - 1)
            / MILLIS_PER_SECOND,
    )
    .map_err(|_overflow| ErrorKind::Balance.into())
}

fn observations(
    attempt: &Attempt,
    reported: &garde::Valid<Meter>,
    totals: &Totals,
    now: jiff::Timestamp,
) -> Result<Duration> {
    let runtime = Duration::from_millis(reported.runtime_ms);
    let elapsed = now
        .duration_since(attempt.created_at)
        .try_into()
        .unwrap_or(Duration::ZERO);
    if runtime < attempt.reported_runtime
        || runtime > elapsed
        || i64::from(reported.calls) > totals.calls
        || reported
            .input_tokens
            .is_some_and(|n| i128::from(n) > i128::from(totals.input))
        || reported
            .output_tokens
            .is_some_and(|n| i128::from(n) > i128::from(totals.output))
    {
        return Err(invalid(
            "Authoring usage exceeds receipts or regresses its runtime",
        ));
    }
    Ok(runtime)
}

/// The usage a message reports, from its attempt's dispatch receipts.
fn usage(known: bool, input: i64, output: i64, provider: i64, runtime: i64) -> Result<Usage> {
    Ok(Usage {
        knowledge: if known {
            Knowledge::Known
        } else {
            Knowledge::Unknown
        },
        input_tokens: known
            .then(|| u64::try_from(input))
            .transpose()
            .map_err(|_negative| ErrorKind::Token)?,
        output_tokens: known
            .then(|| u64::try_from(output))
            .transpose()
            .map_err(|_negative| ErrorKind::Token)?,
        provider_cost_nanos: known.then(|| provider.to_string()),
        runtime_cost_nanos: runtime.to_string(),
    })
}

/// A message's usage columns: the dispatch receipts of its attempt, summed in
/// SQL, once a report was acknowledged. All absent before that.
#[derive(sqlx::FromRow)]
pub(crate) struct Metered {
    reported_runtime_ms: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    provider_nanos: Option<i64>,
    known: Option<bool>,
}

impl Metered {
    pub(crate) fn usage(&self) -> Result<Option<Usage>> {
        let Some(runtime_ms) = self.reported_runtime_ms else {
            return Ok(None);
        };
        let runtime = Duration::from_millis(u64::try_from(runtime_ms).unwrap_or(0));
        usage(
            self.known.unwrap_or(false),
            self.input_tokens.unwrap_or(0),
            self.output_tokens.unwrap_or(0),
            self.provider_nanos.unwrap_or(0),
            runtime_nanos(runtime)?,
        )
        .map(Some)
    }
}

fn check_cost(reported: &Meter, authoritative: i64) -> Result<()> {
    if let Some(raw) = &reported.provider_cost_nanos {
        let value = raw
            .parse::<i64>()
            .map_err(|_invalid| invalid("Invalid provider nanos"))?;
        if value < 0 || value.to_string() != *raw || value > authoritative {
            return Err(invalid("Provider cost exceeds daemon receipts"));
        }
    }
    if reported.knowledge == Knowledge::Unknown
        && (reported.input_tokens.is_some()
            || reported.output_tokens.is_some()
            || reported.provider_cost_nanos.is_some())
    {
        return Err(invalid("Unknown provider usage must omit provider figures"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_cost;
    use afd_core::error_code;
    use afd_wire::authoring::{Knowledge, Meter};

    fn meter(knowledge: Knowledge, cost: Option<&str>, tokens: Option<u64>) -> Meter {
        Meter {
            knowledge,
            calls: 1,
            input_tokens: tokens,
            output_tokens: tokens,
            runtime_ms: 1,
            provider_cost_nanos: cost.map(str::to_owned),
        }
    }

    fn refused(reported: &Meter, authoritative: i64) -> Option<afd_core::error_code::ErrorCode> {
        check_cost(reported, authoritative)
            .err()
            .map(|error| error.code())
    }

    #[test]
    fn a_reported_provider_cost_is_a_canonical_figure_within_the_receipts() {
        let known = |cost| meter(Knowledge::Known, Some(cost), Some(1));
        assert_eq!(refused(&known("40"), 40), None);
        for cost in ["41", "-1", "040", "+4"] {
            assert_eq!(
                refused(&known(cost), 40),
                Some(error_code::INVALID_REQUEST),
                "{cost} against 40 nanos of receipts"
            );
        }
    }

    #[test]
    fn unknown_usage_reports_no_provider_figure() {
        assert_eq!(refused(&meter(Knowledge::Unknown, None, None), 0), None);
        for reported in [
            meter(Knowledge::Unknown, None, Some(1)),
            Meter {
                output_tokens: Some(1),
                ..meter(Knowledge::Unknown, None, None)
            },
            meter(Knowledge::Unknown, Some("0"), None),
        ] {
            assert_eq!(
                refused(&reported, 0),
                Some(error_code::INVALID_REQUEST),
                "{reported:?}"
            );
        }
    }
}
