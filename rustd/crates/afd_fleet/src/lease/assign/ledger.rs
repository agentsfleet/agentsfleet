//! Claim a slot and its exact oldest unsettled admission in one transaction.

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER};
use afd_observability::metrics::label::fleet::HeldClaim;
use afd_observability::producers;
use jiff::Timestamp;
use sqlx::Connection as _;

use super::{MAX_CANDIDATES_PER_POLL, PollCost, started};
use crate::error::{Result, query, row_malformed};
use crate::lease::affinity::{self, Fence};
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::reclaim::Reused;
use crate::lease::sql::{self, queue};
use crate::lease::store::Leases;

/// Query context shared by the claim's transaction operations.
const CONTEXT: &str = "admissions ledger claim";
/// Table named by a malformed stored identifier.
const LEDGER: &str = "fleet.admissions";

/// The admission row whose slot and fence the transaction just won.
#[derive(Debug, sqlx::FromRow)]
struct Admission {
    event_id: String,
    workspace_id: String,
    actor: String,
    event_type: String,
    request_json: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    event_created_at: Timestamp,
    redelivered: bool,
    fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    lease_expires_at: Timestamp,
    held: bool,
    holder: Option<String>,
    reused_tenant_id: Option<String>,
    reused_posture: Option<String>,
    reused_model: Option<String>,
}

impl Leases {
    /// A bounded candidate scan followed by atomic slot-and-head claims.
    pub(super) async fn ledger_counted(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let hint = self.doorbell().take();
        let held: Vec<&str> = held.iter().map(Uuid7::as_str).collect();
        let mut connection = self.pool().acquire().await?;
        cost.database_roundtrips += 1;
        let candidates: Vec<String> = sqlx::query_scalar(queue::SELECT_CANDIDATES)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(runner.as_str())
            .bind(i64::try_from(MAX_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(&held)
            .bind(hint.as_ref().map(Uuid7::as_str))
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        cost.candidates_scanned = u64::try_from(candidates.len()).unwrap_or(u64::MAX);
        for fleet in candidates {
            let fleet = Uuid7::parse(&fleet).map_err(row_malformed(LEDGER, "fleet_id"))?;
            if let Some(acquired) = self
                .ledger_claim(&mut connection, &fleet, runner, cost)
                .await?
            {
                producers::fleet::run_started(started(acquired.kind));
                affinity::count_resumed(&acquired);
                return Ok(Some(acquired));
            }
        }
        Ok(None)
    }

    /// Roll back the slot too when the head is parked, locked, or gone.
    async fn ledger_claim(
        &self,
        connection: &mut sqlx::PgConnection,
        fleet: &Uuid7,
        runner: &Uuid7,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        cost.database_roundtrips += 1;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        cost.database_roundtrips += 1;
        let row: Option<Admission> = sqlx::query_as(queue::CLAIM_HEAD)
            .bind(fleet.as_str())
            .bind(runner.as_str())
            .bind(LEASE_TTL)
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_EXPIRED)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT))?;
        let Some(row) = row else {
            cost.database_roundtrips += 1;
            transaction.rollback().await.map_err(query(CONTEXT))?;
            return Ok(None);
        };
        let displaced_hold = row.held && row.holder.as_deref() != Some(runner.as_str());
        let acquired = row.acquired(fleet, runner)?;
        cost.database_roundtrips += 1;
        transaction.commit().await.map_err(query(CONTEXT))?;
        if displaced_hold {
            producers::fleet::hold::claimed(HeldClaim::OtherAfterLapse);
        }
        Ok(Some(acquired))
    }
}

impl Admission {
    /// Build the same downstream envelope from the committed ledger payload.
    fn acquired(self, fleet: &Uuid7, runner: &Uuid7) -> Result<Acquired> {
        // All three are non-null on a matched prior lease; the outer join
        // makes all three absent when the admission has never been issued.
        let reused = self
            .reused_tenant_id
            .zip(self.reused_posture)
            .zip(self.reused_model)
            .map(|((tenant_id, posture), model)| Reused {
                tenant_id,
                posture,
                model,
            });
        let kind = if reused.is_some() {
            Kind::Reclaim
        } else {
            Kind::Fresh
        };
        Ok(Acquired {
            fleet_id: fleet.clone(),
            fence: Fence::from_i64(self.fencing_token),
            leased_until: self.lease_expires_at,
            kind,
            event_id: self.event_id,
            actor: self.actor,
            event_type: self.event_type,
            request_json: self.request_json,
            workspace_id: Uuid7::parse(&self.workspace_id)
                .map_err(row_malformed(LEDGER, "workspace_id"))?,
            event_created_at: self.event_created_at,
            reused,
            resume_hold: self.held
                && self.holder.as_deref() == Some(runner.as_str())
                && kind == Kind::Fresh,
            redelivered: self.redelivered,
        })
    }
}
