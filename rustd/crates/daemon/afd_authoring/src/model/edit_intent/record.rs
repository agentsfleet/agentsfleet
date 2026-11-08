//! Account the physical check even when its answer cannot affect the candidate.

use af_core::id::Uuid7;
use af_wire::{decision::DecisionUsage, usage::UsageLine};
use afd_billing::{
    Posture,
    biller::{Account, Attribution, Settlement, WorkKey},
    price::{Rater, Window},
};
use sqlx::Connection as _;

use super::{
    CONTEXT, ModelCalls, Prepared,
    verdict::{Disposition, Evaluation},
};
use crate::{Result, error::raise::query, lease::guard};

impl ModelCalls {
    pub(super) async fn record_edit(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        counters: &guard::Counters,
        prepared: &Prepared,
        evaluation: Evaluation,
    ) -> Result<Disposition> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let fresh = match guard::publication(&mut tx, runner, lease, counters).await {
            Ok(current) => {
                current.draft.candidate_revision == prepared.candidate_revision
                    && afd_runner::decision_assignment_on(&mut tx, runner)
                        .await?
                        .is_some_and(|selected| {
                            *selected.assignment == prepared.snapshot.assignment
                        })
            }
            Err(error)
                if matches!(
                    error.code(),
                    af_core::error_code::AUTHORING_FENCE
                        | af_core::error_code::AUTHORING_NOT_FOUND
                        | af_core::error_code::AUTHORING_MODEL
                ) =>
            {
                false
            }
            Err(error) => return Err(error),
        };
        let disposition = if fresh {
            evaluation.disposition
        } else {
            Disposition::Stale
        };
        let usage = evaluation.usage.or_else(|| {
            (!evaluation.sent).then_some(DecisionUsage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
            })
        });
        let sequence: i64 = sqlx::query_scalar(include_str!("sql/sequence.sql"))
            .bind(&prepared.request.call_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let settlement = usage.and_then(|usage| settlement(prepared, usage, sequence).ok());
        let charged = settlement
            .as_ref()
            .and_then(|bill| bill.lines.first())
            .map(|line| line.charged.as_i64());
        sqlx::query(include_str!("sql/record.sql"))
            .bind(&prepared.request.call_id)
            .bind(disposition.as_str())
            .bind(usage.map(sqlx::types::Json))
            .bind(charged)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        if let Some(bill) = &settlement {
            afd_billing::ledger::log_lines(
                &mut *tx,
                &self.drafts.entropy,
                prepared.snapshot.at,
                bill,
            )
            .await?;
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        tracing::info!(event = "setup_edit_intent", dispatch_id = %prepared.request.call_id,
            disposition = disposition.as_str(), usage_known = charged.is_some());
        Ok(disposition)
    }
}

fn settlement(prepared: &Prepared, usage: DecisionUsage, sequence: i64) -> Result<Settlement> {
    let snapshot = &prepared.snapshot;
    let line = UsageLine {
        seq: u32::try_from(sequence).map_err(|_invalid| crate::error::ErrorKind::Token)?,
        at: snapshot.at,
        input_tokens: usage
            .uncached_input()
            .ok_or(crate::error::ErrorKind::Token)?,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: usage.output_tokens,
        runtime_ms: 0,
    };
    let rated = Rater::new(
        &snapshot.book,
        Posture::Platform,
        Window {
            from: snapshot.at,
            until: snapshot.at,
        },
    )
    .rate(&line)?;
    if rated.unpriced_tokens != 0 {
        return Err(crate::error::ErrorKind::Model.into());
    }
    Ok(Settlement {
        work: WorkKey::AuthoringAttempt(prepared.attempt.id.clone()),
        payer: Account {
            tenant: prepared.attempt.tenant_id.clone(),
            workspace: prepared.attempt.workspace_id.clone(),
        },
        attribution: Attribution::SetupChat,
        provider: snapshot.provider.clone(),
        model: snapshot.assignment.profile.model.clone(),
        posture: Posture::Platform,
        lines: vec![rated],
    })
}
