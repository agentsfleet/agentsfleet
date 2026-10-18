//! The usage log: priced lines written into `billing.usage_lines`.

use afd_crypto::entropy::Entropy;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use super::INSERT_LINES;
use crate::Result;
use crate::biller::{Attribution, Settlement};
use crate::error::{charge_overflow, query};
use crate::price::RatedLine;

/// What the statement was doing, for [`query`].
const CONTEXT_LOG: &str = "log usage lines";

/// Writes `settlement`'s lines, unsettled, on `executor`.
///
/// Called inside the WORK's transaction, so a run and what it cost commit or
/// roll back together; a biller settles them after commit. A line already
/// logged under its `(work, seq)` is left as it is, so a retried report and the
/// built-in biller's own call both land each line once.
///
/// # Errors
/// The statement would not run, or a row id could not be minted.
pub async fn log_lines<'e>(
    executor: impl PgExecutor<'e>,
    entropy: &Entropy,
    now: Timestamp,
    settlement: &Settlement,
) -> Result<()> {
    if settlement.lines.is_empty() {
        return Ok(());
    }
    let ids = settlement
        .lines
        .iter()
        .map(|_line| Ok(entropy.uuid7(now)?.as_str().to_owned()))
        .collect::<Result<Vec<String>>>()?;
    let (fleet, fleet_name, event_id, attempt) = match &settlement.attribution {
        Attribution::Fleet {
            fleet,
            fleet_name,
            event_id,
        } => (
            Some(fleet.as_str()),
            Some(fleet_name.as_str()),
            Some(event_id.as_str()),
            None,
        ),
        Attribution::SetupChat => (None, None, None, Some(settlement.work.id().as_str())),
    };
    let lines = &settlement.lines;
    let unpriced = lines
        .iter()
        .map(|line| i64::try_from(line.unpriced_tokens).map_err(|_overflow| charge_overflow()))
        .collect::<Result<Vec<i64>>>()?;
    sqlx::query(INSERT_LINES)
        .bind(ids)
        .bind(settlement.work.work_type())
        .bind(settlement.work.id().as_str())
        .bind(settlement.payer.tenant.as_str())
        .bind(settlement.payer.workspace.as_str())
        .bind(fleet)
        .bind(fleet_name)
        .bind(event_id)
        .bind(attempt)
        .bind(&settlement.provider)
        .bind(&settlement.model)
        .bind(settlement.posture.as_str())
        .bind(column(lines, |line| i64::from(line.usage.seq)))
        .bind(column(lines, |line| line.usage.at.to_sqlx()))
        .bind(column(lines, |line| i64::from(line.usage.input_tokens)))
        .bind(column(lines, |line| {
            i64::from(line.usage.cached_input_tokens)
        }))
        .bind(column(lines, |line| {
            i64::from(line.usage.cache_write_tokens)
        }))
        .bind(column(lines, |line| {
            i64::from(line.usage.cache_write_5m_tokens)
        }))
        .bind(column(lines, |line| {
            i64::from(line.usage.cache_write_1h_tokens)
        }))
        .bind(column(lines, |line| i64::from(line.usage.output_tokens)))
        .bind(column(lines, |line| i64::from(line.usage.runtime_ms)))
        .bind(column(lines, |line| {
            line.price.as_ref().map(|id| id.as_str().to_owned())
        }))
        .bind(unpriced)
        .bind(column(lines, |line| line.charged.as_i64()))
        .execute(executor)
        .await
        .map_err(query(CONTEXT_LOG))?;
    Ok(())
}

/// One column of `lines`, for an `unnest` bind.
fn column<T>(lines: &[RatedLine], field: impl Fn(&RatedLine) -> T) -> Vec<T> {
    lines.iter().map(field).collect()
}
