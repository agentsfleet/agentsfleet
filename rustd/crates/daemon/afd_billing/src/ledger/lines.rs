//! The usage log: priced lines written into `billing.usage_lines`.

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use super::INSERT_LINES;
use crate::biller::{Attribution, Settlement};
use crate::error::{charge_overflow, query};
use crate::price::RatedLine;
use crate::{Nanos, Result};

/// What the statement was doing, for [`query`].
const CONTEXT_LOG: &str = "log usage lines";

/// Writes `settlement`'s lines on `executor`, charging each new one.
///
/// Called inside the WORK's transaction, so a run and what it cost commit or
/// roll back together. The insert debits the payer's wallet by the new lines'
/// cost and adds them to the daily rollups in the same statement (the
/// `billing.usage_lines` triggers). A line already logged under its
/// `(work, kind, seq)` is left as it is and charged nothing, so a retried
/// report lands and charges each line once.
/// Returns only newly inserted coding-request cost, excluding runtime and
/// decision lines. Task accounting can consume this exact delta
/// without rescanning history or assuming requests finish in sequence order.
///
/// # Errors
/// The statement would not run, or a row id could not be minted.
pub async fn log_lines<'e>(
    executor: impl PgExecutor<'e>,
    entropy: &Entropy,
    now: Timestamp,
    settlement: &Settlement,
) -> Result<Nanos> {
    if settlement.lines.is_empty() {
        return Ok(Nanos::ZERO);
    }
    let ids = settlement
        .lines
        .iter()
        .map(|_line| Ok(entropy.uuid7(now)?))
        .collect::<Result<Vec<Uuid7>>>()?;
    let (fleet, fleet_name, turn_id, run) = match &settlement.attribution {
        Attribution::Fleet {
            fleet,
            fleet_name,
            turn_id,
        } => (Some(fleet), Some(fleet_name.as_str()), Some(turn_id), None),
        Attribution::SetupRun { message } => {
            (None, None, Some(message), Some(settlement.work.id()))
        }
    };
    let lines = &settlement.lines;
    let unpriced = lines
        .iter()
        .map(|line| i64::try_from(line.unpriced_tokens).map_err(|_overflow| charge_overflow()))
        .collect::<Result<Vec<i64>>>()?;
    let model_nanos: i64 = sqlx::query_scalar(INSERT_LINES)
        .bind(ids)
        .bind(settlement.work.work_type())
        .bind(settlement.work.id())
        .bind(&settlement.payer.tenant)
        .bind(&settlement.payer.workspace)
        .bind(fleet)
        .bind(fleet_name)
        .bind(turn_id)
        .bind(run)
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
        .bind(column(lines, |line| line.price.clone()))
        .bind(unpriced)
        .bind(column(lines, |line| line.charged.as_i64()))
        .bind(column(lines, |line| line.usage.kind.as_str()))
        .bind(af_wire::usage::LineKind::Model.as_str())
        .fetch_one(executor)
        .await
        .map_err(query(CONTEXT_LOG))?;
    Ok(Nanos::from_i64(model_nanos))
}

/// One column of `lines`, for an `unnest` bind.
fn column<T>(lines: &[RatedLine], field: impl Fn(&RatedLine) -> T) -> Vec<T> {
    lines.iter().map(field).collect()
}

#[cfg(test)]
mod tests {
    use af_core::id::{ENTROPY_LEN, Uuid7};
    use af_crypto::entropy::Entropy;
    use af_wire::usage::{LineKind, UsageLine};
    use jiff::Timestamp;

    use super::log_lines;
    use crate::Nanos;
    use crate::biller::{Account, Attribution, Settlement, WorkKey};
    use crate::error::ErrorKind;
    use crate::price::RatedLine;
    use crate::rates::Posture;

    type Checked = Result<(), Box<dyn std::error::Error>>;

    /// A server nothing listens on: a statement sent here fails to connect.
    const NOWHERE: &str = "postgres://nobody@127.0.0.1:1/nowhere";

    /// A setup run's settlement of `lines`, every identifier `id`.
    fn settlement(id: &Uuid7, lines: Vec<RatedLine>) -> Settlement {
        Settlement {
            work: WorkKey::SetupRun(id.clone()),
            payer: Account {
                tenant: id.clone(),
                workspace: id.clone(),
            },
            attribution: Attribution::SetupRun {
                message: id.clone(),
            },
            provider: String::new(),
            model: String::new(),
            posture: Posture::Platform,
            lines,
        }
    }

    /// One model line carrying `unpriced_tokens`.
    const fn line(unpriced_tokens: u64) -> RatedLine {
        RatedLine {
            usage: UsageLine {
                kind: LineKind::Model,
                seq: 1,
                at: Timestamp::UNIX_EPOCH,
                input_tokens: 10,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
                output_tokens: 5,
                runtime_ms: 0,
            },
            price: None,
            charged: Nanos::ZERO,
            unpriced_tokens,
        }
    }

    /// A settlement with no lines writes nothing, so it never reaches the
    /// datastore: against a server nothing listens on it still answers `Ok`.
    #[tokio::test]
    async fn a_settlement_with_no_lines_writes_nothing() -> Checked {
        let pool = sqlx::PgPool::connect_lazy(NOWHERE)?;
        let id = Uuid7::encode(Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])?;
        let logged = log_lines(
            &pool,
            &Entropy::new(),
            Timestamp::UNIX_EPOCH,
            &settlement(&id, Vec::new()),
        )
        .await;
        assert!(
            logged.is_ok(),
            "an empty settlement logs nothing: {logged:?}"
        );
        Ok(())
    }

    /// A line whose row id cannot be minted is refused before any statement:
    /// against a server nothing listens on, the refusal is the mint's.
    #[tokio::test]
    async fn a_line_whose_id_cannot_be_minted_is_refused_unsent() -> Checked {
        let pool = sqlx::PgPool::connect_lazy(NOWHERE)?;
        let id = Uuid7::encode(Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])?;
        let (entropy, control) = Entropy::new_mocked();
        control.fail_next();
        let refused = log_lines(
            &pool,
            &entropy,
            Timestamp::UNIX_EPOCH,
            &settlement(&id, vec![line(0)]),
        )
        .await
        .err()
        .ok_or("an unminted line is refused")?;
        assert!(
            matches!(refused.kind(), ErrorKind::Entropy { .. }),
            "{refused:?}"
        );
        assert_eq!(
            refused.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED
        );
        Ok(())
    }

    /// Unpriced tokens past the column's range are refused before any
    /// statement, never wrapped into a negative count.
    #[tokio::test]
    async fn unpriced_tokens_past_the_column_range_are_refused_unsent() -> Checked {
        let pool = sqlx::PgPool::connect_lazy(NOWHERE)?;
        let id = Uuid7::encode(Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])?;
        let past = u64::try_from(i64::MAX)? + 1;
        let refused = log_lines(
            &pool,
            &Entropy::new(),
            Timestamp::UNIX_EPOCH,
            &settlement(&id, vec![line(past)]),
        )
        .await
        .err()
        .ok_or("an overflowing count is refused")?;
        assert!(
            matches!(refused.kind(), ErrorKind::ChargeOverflow),
            "{refused:?}"
        );
        Ok(())
    }
}
