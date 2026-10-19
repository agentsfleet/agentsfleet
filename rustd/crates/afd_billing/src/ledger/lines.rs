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

#[cfg(test)]
mod tests {
    use afd_core::id::{ENTROPY_LEN, Uuid7};
    use afd_crypto::entropy::Entropy;
    use jiff::Timestamp;

    use super::log_lines;
    use crate::biller::{Account, Attribution, Settlement, WorkKey};
    use crate::rates::Posture;

    /// A server nothing listens on: a statement sent here fails to connect.
    const NOWHERE: &str = "postgres://nobody@127.0.0.1:1/nowhere";

    /// A settlement with no lines writes nothing, so it never reaches the
    /// datastore: against a server nothing listens on it still answers `Ok`.
    #[tokio::test]
    async fn a_settlement_with_no_lines_writes_nothing() {
        let pool = sqlx::PgPool::connect_lazy(NOWHERE)
            .unwrap_or_else(|_malformed| unreachable!("the fixture URL parses"));
        let id = Uuid7::encode(Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])
            .unwrap_or_else(|_unencodable| unreachable!("the epoch encodes"));
        let settlement = Settlement {
            work: WorkKey::AuthoringAttempt(id.clone()),
            payer: Account {
                tenant: id.clone(),
                workspace: id,
            },
            attribution: Attribution::SetupChat,
            provider: String::new(),
            model: String::new(),
            posture: Posture::Platform,
            lines: Vec::new(),
        };
        let logged = log_lines(&pool, &Entropy::new(), Timestamp::UNIX_EPOCH, &settlement).await;
        assert!(
            logged.is_ok(),
            "an empty settlement logs nothing: {logged:?}"
        );
    }
}
