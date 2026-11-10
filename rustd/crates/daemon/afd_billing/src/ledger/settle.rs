//! Charging logged lines.

use sqlx::PgConnection;

use super::{CONTEXT_SETTLE, DEBIT_WALLET, SETTLE_LINES};
use crate::biller::{Receipt, Settlement};
use crate::error::{charge_overflow, query};
use crate::{Nanos, Result};

/// The log event for usage settled against a payer with no wallet row.
const EVENT_SETTLED_WITHOUT_WALLET: &str = "billing_settled_without_wallet";

/// Stamps `settlement`'s still-unsettled lines settled, then debits their
/// whole sum from the payer's wallet.
///
/// The `settled_at IS NULL` guard is what makes a repeat call charge nothing.
/// A concurrent pass over the same lines waits on their row locks, then finds
/// them stamped. The debit is the last statement, so the wallet row is locked
/// only from it to the caller's commit.
pub(super) async fn charge(
    connection: &mut PgConnection,
    settlement: &Settlement,
) -> Result<Receipt> {
    let (work, tenant) = (&settlement.work, &settlement.payer.tenant);
    let seqs: Vec<i64> = settlement
        .lines
        .iter()
        .map(|line| i64::from(line.usage.seq))
        .collect();
    let charged: Vec<i64> = sqlx::query_scalar(SETTLE_LINES)
        .bind(work.work_type())
        .bind(work.id())
        .bind(seqs)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?;
    let total = charged
        .iter()
        .try_fold(0_i64, |sum, line| sum.checked_add(*line))
        .ok_or_else(charge_overflow)?;
    if total == 0 {
        return Ok(Receipt {
            settled_lines: charged.len(),
            ..Receipt::default()
        });
    }
    // The debit takes the whole charge, past zero if it must: a balance that
    // cannot cover it is a debt the next grant repays. Only a payer with no
    // wallet row (never provisioned, or erased after the work ran) has nothing
    // to debit, and its lines settle all the same so the relay's oldest-first
    // scan never stalls behind them.
    let debited = sqlx::query(DEBIT_WALLET)
        .bind(tenant)
        .bind(total)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?
        .rows_affected()
        > 0;
    if !debited {
        // Bound before the macro, which repeats its field expressions in a
        // `log` fallback no process here runs.
        let (tenant_id, work_type, work_id) =
            (tenant.as_str(), work.work_type(), work.id().as_str());
        tracing::warn!(
            tenant_id,
            work_type,
            work_id,
            charged_nanos = total,
            event = EVENT_SETTLED_WITHOUT_WALLET,
            "settled usage for a payer with no wallet; nothing was debited"
        );
    }
    Ok(Receipt {
        settled_lines: charged.len(),
        charged: Nanos::from_i64(total),
        taken: Nanos::from_i64(if debited { total } else { 0 }),
    })
}
