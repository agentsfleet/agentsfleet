//! Charging logged lines.

use af_core::id::Uuid7;
use af_wire::usage::LineKind;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use super::{ADD_USAGE_DAILY, CONTEXT_SETTLE, COUNT_RUN, DEBIT_WALLET, SETTLE_LINES};
use crate::biller::{Receipt, Settlement, WorkKey};
use crate::error::{charge_overflow, query};
use crate::{Nanos, Result};

/// The log event for usage settled against a payer with no wallet row.
const EVENT_SETTLED_WITHOUT_WALLET: &str = "billing_settled_without_wallet";
const EVENT_RUN_COUNTED: &str = "billing_run_count_checked";

/// A line this settlement stamped settled, as the ledger holds it.
#[derive(Debug, sqlx::FromRow)]
struct SettledLine {
    tenant_id: Uuid7,
    fleet_id: Option<Uuid7>,
    fleet_name: Option<String>,
    model: String,
    kind: String,
    seq: i64,
    created_at: jiff_sqlx::Timestamp,
    charged_nanos: i64,
    input_token_count: i64,
    output_token_count: i64,
    runtime_ms: i64,
    unpriced_token_count: i64,
}

/// Stamps `settlement`'s still-unsettled lines settled, adds them to the daily
/// usage rows, counts the run if they hold its first runtime slice, then debits
/// their whole sum from the payer's wallet.
///
/// The `settled_at IS NULL` guard is what makes a repeat call charge nothing,
/// and the daily rows and the run count read only the lines it stamped, so they
/// are added exactly once with the charge. A concurrent pass over the same lines
/// waits on their row locks, then finds them stamped. The statements run in one
/// fixed order, with the daily rows written in key order, so two settlements
/// take their row locks in the same order. The debit is the last statement, so
/// the wallet row is locked only from it to the caller's commit.
pub(super) async fn charge(
    connection: &mut PgConnection,
    settlement: &Settlement,
) -> Result<Receipt> {
    let (work, tenant) = (&settlement.work, &settlement.payer.tenant);
    let (kinds, seqs): (Vec<&str>, Vec<i64>) = settlement
        .lines
        .iter()
        .map(|line| (line.usage.kind.as_str(), i64::from(line.usage.seq)))
        .unzip();
    let settled: Vec<SettledLine> = sqlx::query_as(SETTLE_LINES)
        .bind(work.work_type())
        .bind(work.id())
        .bind(kinds)
        .bind(seqs)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?;
    if settled.is_empty() {
        return Ok(Receipt::default());
    }
    add_usage_daily(connection, work, &settled).await?;
    count_run(connection, work, &settled).await?;
    let total = settled
        .iter()
        .try_fold(0_i64, |sum, line| sum.checked_add(line.charged_nanos))
        .ok_or_else(charge_overflow)?;
    debit(connection, work, tenant, settled.len(), total).await
}

/// Debits `total` from `tenant`'s wallet for the `lines` just settled.
///
/// The debit takes the whole charge, past zero if it must: a balance that
/// cannot cover it is a debt the next grant repays. Only a payer with no wallet
/// row (never provisioned, or erased after the work ran) has nothing to debit,
/// and its lines settle all the same so the relay's oldest-first scan never
/// stalls behind them.
async fn debit(
    connection: &mut PgConnection,
    work: &WorkKey,
    tenant: &Uuid7,
    lines: usize,
    total: i64,
) -> Result<Receipt> {
    if total == 0 {
        return Ok(Receipt {
            settled_lines: lines,
            ..Receipt::default()
        });
    }
    // `exhausted_at` keeps the first crossing's instant while the balance stays
    // at or below zero and clears once a debit leaves credit, so the debit
    // answers true on exactly the one that crossed. `None` is no wallet row.
    let exhausted: Option<bool> = sqlx::query_scalar(DEBIT_WALLET)
        .bind(tenant)
        .bind(total)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?;
    let debited = exhausted.is_some();
    if !debited {
        // Bound before the macro, which repeats its field expressions in a
        // `log` fallback no process here runs.
        let (tenant_id, work_type, work_id) =
            (tenant.as_str(), work.work_type(), work.id().as_str());
        {
            let (charged_nanos, event, message) = (
                &total,
                &EVENT_SETTLED_WITHOUT_WALLET,
                format_args!("settled usage for a payer with no wallet; nothing was debited"),
            );
            tracing::warn!(message, tenant_id, work_type, work_id, charged_nanos, event);
        };
    }
    Ok(Receipt {
        settled_lines: lines,
        charged: Nanos::from_i64(total),
        taken: Nanos::from_i64(if debited { total } else { 0 }),
        exhausted: exhausted.unwrap_or_default(),
    })
}

/// Adds the lines just settled into their daily usage rows.
async fn add_usage_daily(
    connection: &mut PgConnection,
    work: &WorkKey,
    settled: &[SettledLine],
) -> Result<()> {
    sqlx::query(ADD_USAGE_DAILY)
        .bind(work.work_type())
        .bind(column(settled, |line| line.tenant_id.clone()))
        .bind(column(settled, |line| line.fleet_id.clone()))
        .bind(column(settled, |line| line.fleet_name.clone()))
        .bind(column(settled, |line| line.model.clone()))
        .bind(column(settled, |line| line.created_at.to_jiff().to_sqlx()))
        .bind(column(settled, |line| line.charged_nanos))
        .bind(column(settled, |line| line.input_token_count))
        .bind(column(settled, |line| line.output_token_count))
        .bind(column(settled, |line| line.runtime_ms))
        .bind(column(settled, |line| line.unpriced_token_count))
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?;
    Ok(())
}

/// Counts the run when the lines just settled hold its first runtime slice;
/// `count_run.sql` checks that no smaller slice exists anywhere in the ledger.
async fn count_run(
    connection: &mut PgConnection,
    work: &WorkKey,
    settled: &[SettledLine],
) -> Result<()> {
    let runtime = LineKind::Runtime.as_str();
    let slices: Vec<&SettledLine> = settled.iter().filter(|line| line.kind == runtime).collect();
    let Some(first) = slices.iter().min_by_key(|line| line.seq) else {
        return Ok(());
    };
    let counted = sqlx::query(COUNT_RUN)
        .bind(work.work_type())
        .bind(work.id())
        .bind(&first.tenant_id)
        .bind(first.fleet_id.as_ref())
        .bind(runtime)
        .bind(first.seq)
        .bind(first.created_at.to_jiff().to_sqlx())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_SETTLE))?
        .rows_affected()
        > 0;
    let (work_id, seq) = (work.id().as_str(), first.seq);
    tracing::debug!(
        work_id,
        seq,
        counted,
        event = EVENT_RUN_COUNTED,
        "settled the run's first runtime slice"
    );
    Ok(())
}

/// One column of `lines`, for an `unnest` bind.
fn column<T>(lines: &[SettledLine], field: impl Fn(&SettledLine) -> T) -> Vec<T> {
    lines.iter().map(field).collect()
}
