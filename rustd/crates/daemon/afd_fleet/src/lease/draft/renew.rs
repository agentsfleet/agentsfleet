//! A draft lease's renewal: the shared extension under the host lock, its
//! usage lines logged in the same transaction, and the payer's funds checked
//! first. Nothing is reserved; only the billing relay moves money.
//!
//! The fleet's renewal batch reads fleet rows, so a session sends a draft's
//! lease here, one statement set per lease, beside its fleet batch.

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use afd_billing::biller::{Account, Ask, Funds, Settlement, WorkKey};
use jiff::Timestamp;
use sqlx::{Connection as _, FromRow as _, PgConnection};

use super::DraftLease;
use crate::error::{
    Result, lease_cancelled, lease_lost, lease_max_runtime, lease_not_found, query,
    renewal_no_credits,
};
use crate::lease::renew::{RenewAnswer, Renewed};
use crate::lease::usage::{DraftSlice, Pricing, SliceColumns};
use crate::lease::{LeaseService, sql};

/// What query failures name a draft renewal as.
const CONTEXT: &str = "renew a draft lease";

/// Logged when a draft lease's payer can no longer fund it.
const EVENT_NO_CREDITS: &str = "draft_lease_no_credits";

impl LeaseService {
    /// Extends `runner`'s draft-owned `lease_id`, logging the slice since its
    /// cursor and the request lines `request` carries.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's, one cancelled, lost or past
    /// its runtime cap, and one whose payer cannot fund another slice, each
    /// with the code a fleet's renewal answers. Known request lines are logged
    /// either way. Reports a datastore that would not answer.
    pub async fn renew_draft(
        &self,
        runner: &Uuid7,
        lease_id: &Uuid7,
        request: &RenewRequest,
        now: Timestamp,
    ) -> Result<Timestamp> {
        let Some(lease) = self.leases.draft_lease(lease_id, runner).await? else {
            return Err(lease_not_found());
        };
        let posture = Pricing::posture_of(&lease.posture)?;
        let pricing = Pricing::load(
            self.leases.pool(),
            posture,
            &lease.provider,
            &lease.model,
            lease_id.as_str(),
        )
        .await;
        let admission = if lease.cancelled {
            Err(lease_cancelled())
        } else if lease.status != sql::LEASE_STATUS_ACTIVE {
            Err(lease_lost())
        } else {
            self.draft_funds(&lease, lease_id, posture, now).await
        };
        let mut connection = self.leases.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let requests = &request.requests.0;
        let renewed = match admission {
            Ok(()) => {
                self.extend_draft(&mut tx, runner, lease_id, &lease, &pricing, requests)
                    .await?
            }
            Err(refusal) => {
                let known = requested(&pricing, lease_id, &lease, requests, now)?;
                self.log_draft(&mut tx, known.as_ref(), now).await?;
                tx.commit().await.map_err(query(CONTEXT))?;
                return Err(refusal);
            }
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        match renewed {
            Renewed::Extended { expires_at, .. } => Ok(expires_at),
            Renewed::Cancelled => Err(lease_cancelled()),
            Renewed::Lost => Err(lease_lost()),
            Renewed::MaxRuntime => Err(lease_max_runtime()),
        }
    }

    /// The host lock, the slot-then-lease locks and the shared extension, with
    /// the slice and the request lines logged beside it. A refused extension
    /// still logs the request lines: usage the runner spent is owed.
    async fn extend_draft(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        runner: &Uuid7,
        lease_id: &Uuid7,
        lease: &DraftLease,
        pricing: &Pricing,
        requests: &[af_wire::usage::RequestUsage],
    ) -> Result<Renewed> {
        let host = self.leases.lock_host_capacity(tx, runner).await?;
        let Some(host) = host.filter(|host| host.can_renew()) else {
            let known = requested(pricing, lease_id, lease, requests, Timestamp::now())?;
            self.log_draft(tx, known.as_ref(), Timestamp::now()).await?;
            return Ok(Renewed::Lost);
        };
        let cancelled: Option<bool> = sqlx::query_scalar(sql::shared::LOCK_CANCELLED)
            .bind(lease_id)
            .bind(runner)
            .fetch_optional(&mut **tx)
            .await
            .map_err(query(CONTEXT))?;
        let refused = match cancelled {
            Some(false) => None,
            Some(true) => Some(Renewed::Cancelled),
            None => Some(Renewed::Lost),
        };
        if let Some(verdict) = refused {
            let known = requested(pricing, lease_id, lease, requests, host.now())?;
            self.log_draft(tx, known.as_ref(), host.now()).await?;
            return Ok(verdict);
        }
        let row = sqlx::query(sql::shared::EXTEND_LEASE)
            .bind(lease_id)
            .bind(runner)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(self.leases.clock().ttl())
            .bind(self.leases.clock().max_runtime())
            .fetch_one(&mut **tx)
            .await
            .map_err(query(CONTEXT))?;
        let answer = RenewAnswer::from_row(&row).map_err(query(CONTEXT))?;
        let slice = SliceColumns::from_row(&row)
            .map_err(query(CONTEXT))?
            .into_draft_slice();
        let Some(expires_at) = answer.extended() else {
            let known = requested(pricing, lease_id, lease, requests, host.now())?;
            self.log_draft(tx, known.as_ref(), host.now()).await?;
            return Ok(answer.verdict(host.now()));
        };
        let settlement = match slice {
            Some(slice) => pricing.draft_settlement(lease_id, slice, requests)?,
            None => requested(pricing, lease_id, lease, requests, host.now())?,
        };
        self.log_draft(tx, settlement.as_ref(), host.now()).await?;
        Ok(Renewed::Extended {
            expires_at,
            settlement: settlement.map(Box::new),
        })
    }

    /// Logs a draft lease's priced lines in the caller's transaction; the
    /// relay settles them.
    pub(super) async fn log_draft(
        &self,
        connection: &mut PgConnection,
        settlement: Option<&Settlement>,
        now: Timestamp,
    ) -> Result<()> {
        if let Some(settlement) = settlement {
            afd_billing::ledger::log_lines(
                &mut *connection,
                self.leases.entropy(),
                now,
                settlement,
            )
            .await?;
        }
        Ok(())
    }

    /// Whether the payer can fund one more slice of `lease`: the balance, less
    /// every logged line the relay has not charged, against the floor one
    /// renewal may spend. A check only. A balance that cannot be read, or a
    /// payer with no wallet, admits the renewal, as a fleet's does.
    async fn draft_funds(
        &self,
        lease: &DraftLease,
        lease_id: &Uuid7,
        posture: afd_billing::rates::Posture,
        now: Timestamp,
    ) -> Result<()> {
        let Ok(estimate) = self
            .accounts
            .estimate(posture, &lease.provider, &lease.model, now)
            .await
        else {
            return Ok(());
        };
        let ask = Ask {
            work: WorkKey::AuthoringAttempt(lease_id.clone()),
            payer: Account {
                tenant: lease.tenant_id.clone(),
                workspace: lease.workspace_id.clone(),
            },
            next: estimate.floor(),
        };
        match self.biller.check(&ask).await {
            Ok(Funds::Short { .. }) => {
                let (draft_id, lease_id) = (lease.draft_id.as_str(), lease_id.as_str());
                {
                    let (event, message) = (
                        &EVENT_NO_CREDITS,
                        format_args!("the tenant's balance can no longer fund this setup turn"),
                    );
                    tracing::warn!(message, draft_id, lease_id, event);
                };
                Err(renewal_no_credits())
            }
            Ok(Funds::Covered | Funds::Unconfirmed) | Err(_) => Ok(()),
        }
    }
}

/// `requests` priced as `lease`'s lines with no run time measured: the lines a
/// refused renewal or report still owes.
pub(super) fn requested(
    pricing: &Pricing,
    lease_id: &Uuid7,
    lease: &DraftLease,
    requests: &[af_wire::usage::RequestUsage],
    until: Timestamp,
) -> Result<Option<Settlement>> {
    if requests.is_empty() {
        return Ok(None);
    }
    let slice = DraftSlice {
        runtime_seq: 0,
        checked_at: until.max(lease.created_at),
        started_at: lease.created_at,
        runtime_ms: 0,
        tenant_id: lease.tenant_id.clone(),
        workspace_id: lease.workspace_id.clone(),
        message_id: lease.event_id.clone(),
    };
    pricing.draft_settlement(lease_id, slice, requests)
}
