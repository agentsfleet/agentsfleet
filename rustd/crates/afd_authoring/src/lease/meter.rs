//! Daemon receipts are authoritative; runner observations only advance runtime.

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
};

pub(crate) const INITIAL_RUNTIME_MS: i64 = 0;
const MILLIS_PER_SECOND: i128 = 1000;
const CONTEXT: &str = "verify authoring cumulative usage";
const TOTALS: &str = "\
SELECT count(*)::bigint AS calls,coalesce(sum((usage->>'input_tokens')::bigint),0)::bigint AS input,
coalesce(sum((usage->>'output_tokens')::bigint),0)::bigint AS output,
coalesce(sum((usage->>'provider_cost_nanos')::bigint),0)::bigint AS provider,
coalesce(bool_and(usage->>'knowledge'='known'),true)
AND count(*) FILTER(WHERE status IN ('pending','unknown'))=0 AS known,
count(*) FILTER(WHERE status='failed')::bigint AS failed_calls
FROM fleet.authoring_dispatches WHERE attempt_id=$1::uuid";

pub(crate) struct Verified {
    pub(crate) calls: i64,
    pub(crate) runtime_ms: i64,
    pub(crate) provider_nanos: i64,
    pub(crate) runtime_nanos: i64,
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
    now: i64,
) -> Result<Verified> {
    let totals: Totals = sqlx::query_as(TOTALS)
        .bind(attempt.id.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    let runtime_ms = observations(attempt, reported, &totals, now)?;
    check_cost(reported, totals.provider)?;
    let runtime_nanos = i64::try_from(
        (i128::from(runtime_ms) * i128::from(RUN_NANOS_PER_SEC) + MILLIS_PER_SECOND - 1)
            / MILLIS_PER_SECOND,
    )
    .map_err(|_overflow| ErrorKind::Balance)?;
    Ok(Verified {
        calls: totals.calls,
        runtime_ms,
        provider_nanos: totals.provider,
        runtime_nanos,
        failed_calls: totals.failed_calls,
        usage: totals.usage(runtime_nanos)?,
    })
}

fn observations(
    attempt: &Attempt,
    reported: &garde::Valid<Meter>,
    totals: &Totals,
    now: i64,
) -> Result<i64> {
    let runtime_ms = i64::try_from(reported.runtime_ms).map_err(|_overflow| ErrorKind::Token)?;
    let elapsed = now
        .saturating_sub(attempt.created_at)
        .max(INITIAL_RUNTIME_MS);
    if runtime_ms < attempt.reported_runtime_ms
        || runtime_ms > elapsed
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
    Ok(runtime_ms)
}

impl Totals {
    fn usage(&self, runtime: i64) -> Result<Usage> {
        Ok(Usage {
            knowledge: if self.known {
                Knowledge::Known
            } else {
                Knowledge::Unknown
            },
            input_tokens: self
                .known
                .then(|| u64::try_from(self.input))
                .transpose()
                .map_err(|_negative| ErrorKind::Token)?,
            output_tokens: self
                .known
                .then(|| u64::try_from(self.output))
                .transpose()
                .map_err(|_negative| ErrorKind::Token)?,
            provider_cost_nanos: self.known.then(|| self.provider.to_string()),
            runtime_cost_nanos: runtime.to_string(),
        })
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
