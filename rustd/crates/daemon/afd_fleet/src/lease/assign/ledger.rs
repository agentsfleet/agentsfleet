//! Claim a slot and its exact oldest unsettled admission in one transaction.

use af_core::id::Uuid7;
use af_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER};
use af_observability::metrics::label::fleet::HeldClaim;
use af_observability::producers;
use jiff::Timestamp;
use sqlx::Connection as _;

use super::{MAX_CANDIDATES_PER_POLL, PollCost, started};
use crate::error::{Result, query};
use crate::lease::affinity::{self, Fence};
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::reclaim::Reused;
use crate::lease::sql::{self, queue};
use crate::lease::store::Leases;

/// Query context shared by the claim's transaction operations.
const CONTEXT: &str = "admissions ledger claim";

/// The admission row whose slot and fence the transaction just won.
#[derive(Debug, sqlx::FromRow)]
struct Admission {
    event_id: Uuid7,
    workspace_id: Uuid7,
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
    holder: Option<Uuid7>,
    reused_tenant_id: Option<Uuid7>,
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
        let held: Vec<&Uuid7> = held.iter().collect();
        let mut connection = self.pool().acquire().await?;
        let candidates: Vec<Uuid7> = sqlx::query_scalar(queue::SELECT_CANDIDATES)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(runner)
            .bind(i64::try_from(MAX_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(&held)
            .bind(hint.as_ref())
            .bind(af_wire::task::CAPABILITY)
            .bind(af_wire::task::TaskState::Ready.as_str())
            .bind(af_wire::task::TaskState::Running.as_str())
            .bind(crate::task::DECISION_POLICY_PATH)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        cost.candidates_scanned = u64::try_from(candidates.len()).unwrap_or(u64::MAX);
        for fleet in candidates {
            if let Some(acquired) = self.ledger_claim(&mut connection, &fleet, runner).await? {
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
    ) -> Result<Option<Acquired>> {
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        let row: Option<Admission> = sqlx::query_as(queue::CLAIM_HEAD)
            .bind(fleet)
            .bind(runner)
            .bind(LEASE_TTL)
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_EXPIRED)
            .bind(af_wire::task::CAPABILITY)
            .bind(af_wire::task::TaskState::Ready.as_str())
            .bind(af_wire::task::TaskState::Running.as_str())
            .bind(crate::task::DECISION_POLICY_PATH)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT))?;
        let Some(row) = row else {
            transaction.rollback().await.map_err(query(CONTEXT))?;
            return Ok(None);
        };
        let displaced_hold = row.held && row.holder.as_ref() != Some(runner);
        let acquired = row.acquired(fleet, runner);
        transaction.commit().await.map_err(query(CONTEXT))?;
        if displaced_hold {
            producers::fleet::hold::claimed(HeldClaim::OtherAfterLapse);
        }
        Ok(Some(acquired))
    }
}

impl Admission {
    /// Build the same downstream envelope from the committed ledger payload.
    fn acquired(self, fleet: &Uuid7, runner: &Uuid7) -> Acquired {
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
        Acquired {
            fleet_id: fleet.clone(),
            fence: Fence::from_i64(self.fencing_token),
            leased_until: self.lease_expires_at,
            kind,
            event_id: self.event_id,
            actor: self.actor,
            event_type: self.event_type,
            request_json: self.request_json,
            workspace_id: self.workspace_id.clone(),
            event_created_at: self.event_created_at,
            reused,
            resume_hold: self.held && self.holder.as_ref() == Some(runner) && kind == Kind::Fresh,
            redelivered: self.redelivered,
        }
    }
}
