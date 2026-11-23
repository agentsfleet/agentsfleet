//! Financial evidence commits before live-state reads; logging its line charges it.

use af_crypto::entropy::Entropy;
use af_wire::{
    decision::DecisionUsage,
    durable_task::{DecisionCallState, DecisionReceipt},
};
use afd_billing::biller::Settlement;
use sqlx::{PgConnection, types::Json};

use super::{
    account::Plan,
    row::{self, Call, Lookup},
    sql,
};
use crate::{Result, durable_task::DurableTaskRefusal, error::query};

pub(super) struct Recording<'a> {
    pub call: &'a Call,
    pub receipt: &'a DecisionReceipt,
    pub digest: &'a str,
}

pub(super) struct Written {
    pub call: Call,
    pub replayed: bool,
    pub durable_task_version: Option<i64>,
}

struct Accounting {
    state: DecisionCallState,
    usage: Option<DecisionUsage>,
    charged: Option<i64>,
    canonical: bool,
    failure: bool,
    settlement: Option<Box<Settlement>>,
}

impl Accounting {
    fn of(plan: Plan) -> Option<Self> {
        match plan {
            Plan::Replay => None,
            Plan::Unknown { usage, canonical } => Some(Self {
                state: DecisionCallState::Unknown,
                usage,
                charged: None,
                canonical,
                failure: !canonical,
                settlement: None,
            }),
            Plan::Known {
                usage,
                charged,
                settlement,
            } => Some(Self {
                state: DecisionCallState::Known,
                usage: Some(usage),
                charged: Some(charged.as_i64()),
                canonical: true,
                failure: false,
                settlement: Some(settlement),
            }),
            Plan::NotSent => Some(Self {
                state: DecisionCallState::NotSent,
                usage: None,
                charged: Some(0),
                canonical: true,
                failure: false,
                settlement: None,
            }),
        }
    }
}

impl Recording<'_> {
    pub(super) async fn write(
        &self,
        connection: &mut PgConnection,
        entropy: &Entropy,
        plan: Plan,
    ) -> Result<Written> {
        let accounting = Accounting::of(plan);
        let replayed = accounting.is_none();
        let durable_task_version = if let Some(accounting) = accounting {
            let at = self.header(connection, &accounting).await?;
            if let Some(settlement) = &accounting.settlement {
                afd_billing::ledger::log_lines(&mut *connection, entropy, at, settlement).await?;
            }
            // UPDATE never recreates private payloads removed by owner erasure.
            sqlx::query(sql::RECORD_PAYLOAD)
                .bind(&self.call.id)
                .bind(Json(self.receipt))
                .execute(&mut *connection)
                .await
                .map_err(query("save the physical decision receipt"))?;
            self.account_durable_task(connection, accounting.charged)
                .await?
        } else {
            None
        };
        let saved = row::find(connection, Lookup::Id(&self.call.id))
            .await?
            .ok_or(DurableTaskRefusal::NotFound)?;
        Ok(Written {
            call: saved,
            replayed,
            durable_task_version,
        })
    }

    async fn header(
        &self,
        connection: &mut PgConnection,
        accounting: &Accounting,
    ) -> Result<jiff::Timestamp> {
        let reason = match self.receipt {
            DecisionReceipt::Unknown { cause } => Some(Json(cause)),
            DecisionReceipt::Provider { .. } | DecisionReceipt::NotSent { .. } => None,
        };
        let model = match self.receipt {
            DecisionReceipt::Provider { result } => result.reported_model.as_deref(),
            DecisionReceipt::Unknown { .. } | DecisionReceipt::NotSent { .. } => None,
        };
        let at: jiff_sqlx::Timestamp = sqlx::query_scalar(sql::RECORD)
            .bind(&self.call.id)
            .bind(accounting.state.as_str())
            .bind(accounting.charged.is_some())
            .bind(accounting.usage.map(Json))
            .bind(accounting.charged)
            .bind(accounting.canonical.then_some(self.digest))
            .bind(accounting.failure.then_some(self.digest))
            .bind(reason)
            .bind(model)
            .fetch_one(connection)
            .await
            .map_err(query("record original-owner decision accounting"))?;
        Ok(at.to_jiff())
    }

    async fn account_durable_task(
        &self,
        connection: &mut PgConnection,
        charged: Option<i64>,
    ) -> Result<Option<i64>> {
        let Some(task) = self.call.durable_task_id.as_ref() else {
            return Ok(None);
        };
        // A physical call counts once, at its first report; its charge lands when known.
        let first = self.call.canonical_receipt_digest.is_none()
            && self.call.failure_receipt_digest.is_none();
        if !first && charged.is_none() {
            return Ok(None);
        }
        sqlx::query_scalar(sql::ACCOUNT_DURABLE_TASK)
            .bind(crate::durable_task::guard::wire(task.as_str())?)
            .bind(&self.call.fleet_id)
            .bind(&self.call.workspace_id)
            .bind(charged.unwrap_or(0))
            .bind(i64::MAX)
            .bind(first)
            .fetch_optional(connection)
            .await
            .map_err(query(
                "account cumulative task decision cost without restoring erased state",
            ))
    }
}
