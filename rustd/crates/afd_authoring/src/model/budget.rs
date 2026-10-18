//! Checked integer bounds use per-dispatch upward rounding, independent of runtime billing.

use std::time::Duration;

use afd_billing::RUN_NANOS_PER_SEC;
use afd_credential::provider::RegisteredModel;
use sqlx::PgConnection;

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::{Status, guard::Attempt},
};

pub(crate) const INPUT_PER_CALL: i64 = 32_768;
pub(crate) const OUTPUT_PER_TURN: i64 = 8_192;
pub(crate) const MAX_CALLS: i64 = 8;
pub(crate) const PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a provider's private result outlives its dispatch.
pub(crate) const PRIVATE_TTL: Duration = afd_core::timing::DAY;
/// The longest an authoring attempt runs, renewal included.
pub(crate) const WALL: Duration = Duration::from_secs(300);
const MILLION: i128 = 1_000_000;

/// The owner's balance against every attempt that still holds a reservation.
///
/// `$1` tenant, `$2` and `$3` the statuses whose reservation is still held.
pub(crate) const COVERAGE: &str = include_str!("sql/coverage.sql");

/// [`COVERAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
pub(crate) struct Coverage {
    pub(crate) balance_nanos: i64,
    pub(crate) open_count: i64,
    pub(crate) largest_reserved: i64,
}

/// Reads [`COVERAGE`] for `tenant`; `None` when the tenant has no wallet.
pub(crate) async fn reserved(
    connection: &mut PgConnection,
    tenant: &afd_core::id::Uuid7,
    context: &'static str,
) -> Result<Option<Coverage>> {
    sqlx::query_as(COVERAGE)
        .bind(tenant.as_str())
        .bind(Status::Active.as_str())
        .bind(Status::AwaitingAccounting.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(context))
}

pub(crate) fn cost(model: &RegisteredModel, input: i64, output: i64) -> Result<i64> {
    ceil(input, model.input_nanos_per_million)?
        .checked_add(ceil(output, model.output_nanos_per_million)?)
        .ok_or_else(|| ErrorKind::Balance.into())
}

pub(crate) fn bound(model: &RegisteredModel) -> Result<i64> {
    let seconds = i64::try_from(WALL.as_secs()).map_err(|_overflow| ErrorKind::Balance)?;
    let runtime = RUN_NANOS_PER_SEC
        .checked_mul(seconds)
        .ok_or(ErrorKind::Balance)?;
    if !model.platform_managed {
        return Ok(runtime);
    }
    let input = ceil(INPUT_PER_CALL, model.input_nanos_per_million)?
        .checked_mul(MAX_CALLS)
        .ok_or(ErrorKind::Balance)?;
    let output = ceil(OUTPUT_PER_TURN, model.output_nanos_per_million)?
        .checked_add(MAX_CALLS - 1)
        .ok_or(ErrorKind::Balance)?;
    runtime
        .checked_add(input)
        .and_then(|total| total.checked_add(output))
        .ok_or_else(|| ErrorKind::Balance.into())
}

fn ceil(tokens: i64, rate: i64) -> Result<i64> {
    if tokens < 0 || rate < 0 {
        return Err(ErrorKind::Balance.into());
    }
    let numerator = i128::from(tokens) * i128::from(rate);
    i64::try_from((numerator + MILLION - 1) / MILLION)
        .map_err(|_overflow| ErrorKind::Balance.into())
}

pub(crate) async fn coverage(connection: &mut PgConnection, attempt: &Attempt) -> Result<()> {
    let required = bound(&attempt.model_snapshot)?;
    if attempt.reserved_nanos < required {
        return Err(ErrorKind::Balance.into());
    }
    let totals = reserved(connection, &attempt.tenant_id, "cover authoring dispatch")
        .await?
        .ok_or(ErrorKind::Balance)?;
    let total = totals
        .open_count
        .checked_mul(totals.largest_reserved.max(required))
        .ok_or(ErrorKind::Balance)?;
    if totals.balance_nanos < total {
        return Err(ErrorKind::Balance.into());
    }
    Ok(())
}
