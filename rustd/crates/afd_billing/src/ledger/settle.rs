//! Charging logged lines and releasing holds.

use afd_core::id::Uuid7;
use sqlx::PgConnection;

use super::{
    CAPTURE_HOLD, CONTEXT_RELEASE, CONTEXT_SETTLE, DEBIT_WALLET, RELEASE_HOLD, SELECT_WORK_TENANT,
    SETTLE_LINES, lock_wallet,
};
use crate::biller::{Receipt, Settlement, WorkKey};
use crate::error::{charge_overflow, query, row_malformed};
use crate::{Nanos, Result};

/// The table a malformed tenant id is reported against.
const TABLE_HOLDS: &str = "billing.holds";
/// The column a malformed tenant id is reported against.
const COLUMN_TENANT_ID: &str = "tenant_id";

/// Charges `settlement`'s still-unsettled lines, under a wallet lock the caller
/// already holds.
pub(super) async fn charge(
    connection: &mut PgConnection,
    settlement: &Settlement,
) -> Result<Receipt> {
    let seqs: Vec<i64> = settlement
        .lines
        .iter()
        .map(|line| i64::from(line.usage.seq))
        .collect();
    take(
        connection,
        &settlement.work,
        &settlement.payer.tenant,
        Some(seqs),
        CONTEXT_SETTLE,
    )
    .await
}

/// Charges whatever `work` left unsettled, then frees its hold.
///
/// Idempotent: a work with no hold and no unsettled line answers without
/// writing, and a released hold stays released.
pub(super) async fn release(connection: &mut PgConnection, work: &WorkKey) -> Result<()> {
    let tenant: Option<String> = sqlx::query_scalar(SELECT_WORK_TENANT)
        .bind(work.work_type())
        .bind(work.id().as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_RELEASE))?;
    let Some(tenant) = tenant else {
        return Ok(());
    };
    let tenant = Uuid7::parse(&tenant).map_err(row_malformed(TABLE_HOLDS, COLUMN_TENANT_ID))?;
    lock_wallet(connection, &tenant, CONTEXT_RELEASE).await?;
    take(connection, work, &tenant, None, CONTEXT_RELEASE).await?;
    sqlx::query(RELEASE_HOLD)
        .bind(work.work_type())
        .bind(work.id().as_str())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_RELEASE))?;
    Ok(())
}

/// Stamps `work`'s unsettled lines (only `seqs`, when given) settled, then
/// debits their sum from the wallet and captures it on the hold.
///
/// The `settled_at IS NULL` guard is what makes a repeat call charge nothing:
/// a line settled once is never returned again.
async fn take(
    connection: &mut PgConnection,
    work: &WorkKey,
    tenant: &Uuid7,
    seqs: Option<Vec<i64>>,
    context: &'static str,
) -> Result<Receipt> {
    let charged: Vec<i64> = sqlx::query_scalar(SETTLE_LINES)
        .bind(work.work_type())
        .bind(work.id().as_str())
        .bind(seqs)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(context))?;
    let total = charged
        .iter()
        .try_fold(0_i64, |sum, line| sum.checked_add(*line))
        .ok_or_else(charge_overflow)?;
    if total > 0 {
        sqlx::query(DEBIT_WALLET)
            .bind(tenant.as_str())
            .bind(total)
            .execute(&mut *connection)
            .await
            .map_err(query(context))?;
        sqlx::query(CAPTURE_HOLD)
            .bind(work.work_type())
            .bind(work.id().as_str())
            .bind(total)
            .execute(&mut *connection)
            .await
            .map_err(query(context))?;
    }
    Ok(Receipt {
        settled_lines: charged.len(),
        charged: Nanos::from_i64(total),
    })
}
