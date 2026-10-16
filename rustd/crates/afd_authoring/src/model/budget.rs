//! Checked integer bounds use per-dispatch upward rounding, independent of runtime billing.

use afd_billing::RUN_NANOS_PER_SEC;
use afd_credential::provider::RegisteredModel;
use sqlx::PgConnection;

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard::Attempt,
};

pub(crate) const INPUT_PER_CALL: i64 = 32_768;
pub(crate) const MAX_CALLS: i64 = 8;
pub(crate) const PROVIDER_TIMEOUT_MS: i64 = 30_000;
pub(crate) const PRIVATE_TTL_MS: i64 = 86_400_000;
const WALL_SECONDS: i64 = 300;
const MILLION: i128 = 1_000_000;
const COVERAGE: &str = "\
SELECT w.balance_nanos, count(a.id)::bigint, coalesce(max(a.reserved_nanos),0)::bigint
FROM billing.tenant_wallet w LEFT JOIN fleet.authoring_attempts a
ON a.tenant_id=w.tenant_id AND (a.status IN ('active','awaiting_accounting') OR a.usage_unknown)
WHERE w.tenant_id=$1::uuid GROUP BY w.balance_nanos";

pub(crate) fn output_limit(model: &RegisteredModel) -> i64 {
    model
        .inference
        .as_ref()
        .map_or(0, |settings| i64::from(settings.max_output_tokens))
}

pub(crate) fn cost(model: &RegisteredModel, input: i64, output: i64) -> Result<i64> {
    ceil(input, model.input_nanos_per_million)?
        .checked_add(ceil(output, model.output_nanos_per_million)?)
        .ok_or_else(|| ErrorKind::Balance.into())
}

pub(crate) fn bound(model: &RegisteredModel) -> Result<i64> {
    let runtime = RUN_NANOS_PER_SEC
        .checked_mul(WALL_SECONDS)
        .ok_or(ErrorKind::Balance)?;
    if !model.platform_managed {
        return Ok(runtime);
    }
    let input = ceil(INPUT_PER_CALL, model.input_nanos_per_million)?
        .checked_mul(MAX_CALLS)
        .ok_or(ErrorKind::Balance)?;
    let output = ceil(output_limit(model), model.output_nanos_per_million)?
        .checked_add(MAX_CALLS - 1)
        .ok_or(ErrorKind::Balance)?;
    runtime
        .checked_add(input)
        .and_then(|total| total.checked_add(output))
        .ok_or_else(|| ErrorKind::Balance.into())
}

fn ceil(tokens: i64, rate: i64) -> Result<i64> {
    let numerator = i128::from(tokens) * i128::from(rate);
    i64::try_from((numerator + MILLION - 1) / MILLION)
        .map_err(|_overflow| ErrorKind::Balance.into())
}

pub(crate) async fn coverage(connection: &mut PgConnection, attempt: &Attempt) -> Result<()> {
    let required = bound(&attempt.model_snapshot)?;
    if attempt.reserved_nanos < required {
        return Err(ErrorKind::Balance.into());
    }
    let totals: Option<(i64, i64, i64)> = sqlx::query_as(COVERAGE)
        .bind(attempt.tenant_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query("cover authoring dispatch"))?;
    let (balance, count, maximum) = totals.ok_or(ErrorKind::Balance)?;
    let total = count
        .checked_mul(maximum.max(required))
        .ok_or(ErrorKind::Balance)?;
    if balance < total {
        return Err(ErrorKind::Balance.into());
    }
    Ok(())
}
