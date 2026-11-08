//! Bounded read-only reconciliation continues without an agent or live owner.

use super::{record, row::Effect};
use crate::{
    Result,
    error::query,
    task::{
        EffectMaintenance, Tasks,
        github::{Reconciled, recovery::Stored},
        guard, transaction,
    },
};
use futures_util::{StreamExt as _, stream};

const CONCURRENT_READS: usize = 4;
const RECHECK_SECONDS: f64 = 60.0;

#[derive(sqlx::FromRow)]
struct Claimed {
    #[sqlx(flatten)]
    effect: Effect,
    #[sqlx(flatten)]
    recovery: Stored,
}

pub(super) async fn sweep(tasks: &Tasks) -> Result<EffectMaintenance> {
    let Some(github) = tasks.dependencies.github.as_ref() else {
        return Ok(EffectMaintenance::default());
    };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let rows: Vec<Claimed> = sqlx::query_as(include_str!("sql/claim_recovery.sql"))
        .bind(crate::task::row::sql_counter(
            CONCURRENT_READS,
            "concurrent evidence reads",
        )?)
        .bind(RECHECK_SECONDS)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("claim bounded exact-effect evidence reads"))?;
    tx.commit()
        .await
        .map_err(query("commit evidence-read claims before network"))?;
    transaction::recycle(connection);
    let mut result = EffectMaintenance::default();
    let mut reads = stream::iter(rows.into_iter().map(|row| async move {
        let id = row.effect.id.clone();
        let outcome = if row.recovery.expires_at <= af_core::clock::now() {
            expire(tasks, &row.effect).await.map(|()| Outcome::Expired)
        } else {
            async {
                let opened = github.open_recovery(
                    &guard::id(&row.effect.workspace_id, "workspace_id")?,
                    &guard::id(&row.effect.id, "effect_id")?,
                    row.recovery,
                )?;
                match opened
                    .session
                    .reconcile_locator(
                        &opened.repository,
                        &opened.branch,
                        opened.repository_id,
                        opened.installation_id,
                        crate::task::github::ReceiptIdentity {
                            effect: &row.effect.id,
                            head: &row.effect.expected_head,
                            tree: &row.effect.tested_tree,
                        },
                    )
                    .await?
                {
                    Reconciled::Confirmed(commit) => {
                        record::confirm(tasks, &row.effect, &commit).await?;
                        Ok(Outcome::Confirmed)
                    }
                    Reconciled::Unknown => Ok(Outcome::Unknown),
                }
            }
            .await
        };
        (id, outcome)
    }))
    .buffer_unordered(CONCURRENT_READS);
    while let Some((id, outcome)) = reads.next().await {
        result.scanned += 1;
        match outcome {
            Ok(Outcome::Confirmed) => result.confirmed += 1,
            Ok(Outcome::Unknown) => result.unknown += 1,
            Ok(Outcome::Expired) => result.expired += 1,
            Err(error) => {
                result.failed += 1;
                tracing::warn!(event = "dt_effect_reconciliation_deferred", effect_id = id, error_code = %error.code());
            }
        }
    }
    Ok(result)
}

enum Outcome {
    Confirmed,
    Unknown,
    Expired,
}

async fn expire(tasks: &Tasks, effect: &Effect) -> Result<()> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    // State precedes recovery-row deletion, as it does in confirmation.
    sqlx::query(include_str!("sql/expire_recovery.sql"))
        .bind(&effect.id)
        .bind("unknown")
        .bind("issued")
        .execute(&mut *tx)
        .await
        .map_err(query("retain uncertainty after read-custody expiry"))?;
    sqlx::query(include_str!("sql/drop_recovery.sql"))
        .bind(&effect.id)
        .execute(&mut *tx)
        .await
        .map_err(query("erase expired effect read custody"))?;
    tx.commit().await.map_err(query(
        "commit read-custody expiry without authorizing a retry",
    ))?;
    transaction::recycle(connection);
    Ok(())
}
