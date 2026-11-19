//! A draft lease's report: one transaction locks the draft, settles the lease
//! with the shared settlement, logs the final usage lines and finishes the
//! draft's message with at most one candidate.
//!
//! Idempotent by lease and fence: the settlement flips the lease only while it
//! is active under its own fence, so a replayed report finds it settled, rolls
//! back and writes nothing a second time. A stale fence is refused and writes
//! nothing; the dead holder's last slice is the expiry sweep's to bill.

use af_core::id::Uuid7;
use af_wire::report::{Outcome, ReportRequest};
use garde::Validate as _;
use jiff::Timestamp;
use sqlx::FromRow as _;

use super::renew::requested;
use crate::error::{Result, lease_lost, lease_not_found, query, stale_fence, wire_lease};
use crate::lease::usage::{Pricing, SliceColumns};
use crate::lease::{LeaseService, sql};

/// What query failures name a draft report as.
const CONTEXT: &str = "settle a draft lease report";

/// [`sql::shared::SETTLE_LEASE`]'s verdict columns, by column name.
#[derive(sqlx::FromRow)]
struct Settled {
    claimed: i64,
    lease_status: Option<String>,
    fence_holds: Option<bool>,
}

impl LeaseService {
    /// Records `runner`'s report for a draft-owned lease: the lease settled,
    /// its last usage lines logged, and the draft's message finished, with one
    /// candidate when the reported setup still stands on the draft's head.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's draft lease, a report for
    /// another message, and a superseded fence, each writing nothing. A
    /// replayed report answers success and writes nothing. Reports a datastore
    /// that would not answer.
    pub async fn report_draft(
        &self,
        creations: &afd_setup::Creations,
        runner: &Uuid7,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) -> Result<()> {
        let lease_id = wire_lease(request.lease_id.as_ref())?;
        let setup = request.setup.as_ref().ok_or_else(lease_not_found)?;
        setup.validate()?;
        let Some(lease) = self.leases.draft_lease(&lease_id, runner).await? else {
            return Err(lease_not_found());
        };
        if request.event_id.as_ref() != lease.event_id.as_str() {
            return Err(lease_not_found());
        }
        if i64::try_from(request.fencing_token).ok() != Some(lease.fencing_token) {
            return Err(stale_fence());
        }
        let pricing = Pricing::load(
            self.leases.pool(),
            Pricing::posture_of(&lease.posture)?,
            &lease.provider,
            &lease.model,
            lease_id.as_str(),
        )
        .await;
        // Owned by the transaction, so its commit can announce after it.
        let mut tx = sqlx::Transaction::begin(self.leases.pool().acquire().await?, None)
            .await
            .map_err(query(CONTEXT))?;
        // The owner first, then the slot and the lease inside the settlement.
        let scope = creations
            .lock_setup_report(&mut tx, &lease.draft_id, &lease.event_id)
            .await?;
        let succeeded = request.outcome == Outcome::Processed;
        let audit_id = self.leases.entropy().uuid7(now)?;
        let metadata = serde_json::json!({
            (afd_runner::sql::meta::LEASE_ID): lease_id.as_str(),
            (afd_runner::sql::meta::AGENTSFLEET_EVENT_ID): lease.event_id.as_str(),
        });
        let (digest, connection_id) = self.leases.host_proof(runner);
        let row = sqlx::query(sql::shared::SETTLE_LEASE)
            .bind(&lease_id)
            .bind(runner)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_REPORTED)
            .bind(succeeded)
            // A draft's lease holds no sandbox, so its report asks for no hold.
            .bind(None::<jiff_sqlx::Timestamp>)
            .bind(self.leases.clock().sandbox_idle_hold())
            .bind(&audit_id)
            .bind(afd_runner::sql::event_type::LEASE_RELEASED)
            .bind(sqlx::types::Json(&metadata))
            .bind(i64::from(af_wire::usage::RUNTIME_MS_MAX))
            .bind(digest)
            .bind(connection_id)
            .bind(None::<Vec<String>>)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let settled = Settled::from_row(&row).map_err(query(CONTEXT))?;
        if settled.claimed == 0 {
            drop(tx);
            return match settled.lease_status.as_deref() {
                // Settled already, by this report's earlier copy: nothing more to write.
                Some(sql::LEASE_STATUS_REPORTED) => Ok(()),
                _ if settled.fence_holds == Some(false) => Err(stale_fence()),
                _ => Err(lease_lost()),
            };
        }
        let requests = &request.requests.0;
        let settlement = match SliceColumns::from_row(&row)
            .map_err(query(CONTEXT))?
            .into_draft_slice()
        {
            Some(slice) => pricing.draft_settlement(&lease_id, slice, requests)?,
            None => requested(&pricing, &lease_id, &lease, requests, now)?,
        };
        self.log_draft(&mut tx, settlement.as_ref(), now).await?;
        let mut owed = afd_setup::setup_lease::SetupOwed::default();
        let announcement = match &scope {
            Some(scope) => {
                let answer = Some(request.response_text.as_ref()).filter(|text| !text.is_empty());
                let finish = afd_setup::setup_lease::SetupFinish {
                    completed: succeeded,
                    answer,
                    proposal: setup.proposal.as_ref(),
                    now,
                };
                Box::pin(creations.finish_setup_turn(&mut tx, scope, finish, &mut owed)).await?
            }
            None => afd_events::activity::Announcement::default(),
        };
        creations
            .commit_setup_report(tx, announcement, owed)
            .await?;
        if let Some(scope) = &scope {
            Box::pin(creations.after_setup_report(scope)).await?;
        }
        Ok(())
    }
}
