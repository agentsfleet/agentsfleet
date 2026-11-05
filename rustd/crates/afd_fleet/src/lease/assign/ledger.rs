//! Claim the oldest ready slot and its head admission in one statement.

use afd_core::id::Uuid7;
use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER};
use afd_observability::metrics::label::fleet::HeldClaim;
use afd_observability::producers;
use jiff::Timestamp;
use sqlx::FromRow as _;
use sqlx::postgres::PgRow;

use super::{Taken, started};
use crate::error::{Result, query, row_malformed};
use crate::lease::affinity::{self, Backoff, Fence};
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::event::Delivery;
use crate::lease::reclaim::Reused;
use crate::lease::sql::{self, queue};
use crate::lease::store::Leases;

/// Query context the claim's failure carries.
const CONTEXT: &str = "admissions ledger claim";
/// Table named by a malformed stored identifier.
const LEDGER: &str = "fleet.admissions";
/// A poll repeated the key of a poll whose reply was lost.
const EVENT_POLL_REPLAYED: &str = "lease_poll_replayed";

/// What both arms of the statement answer: which arm, and the slot it names.
///
/// Decoded first and on its own, so a claim whose full row this daemon cannot
/// read still names the slot it must let go of.
#[derive(Debug, sqlx::FromRow)]
struct Arm {
    replayed_lease: Option<String>,
    replay_freed: bool,
    fleet_id: String,
    fencing_token: i64,
}

/// The admission row whose slot and fence the statement just won, and the
/// event row it opened.
#[derive(Debug, sqlx::FromRow)]
struct Admission {
    fleet_id: String,
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
    opened: bool,
    event_status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    opened_at: Timestamp,
}

impl Leases {
    /// One autocommit statement: replay a repeated key, or claim and open.
    ///
    /// Every write in the statement joins the pick, so an empty claim changes
    /// no row and needs no rollback.
    pub(super) async fn claim_counted(&self, runner: &Uuid7, held: &[Uuid7]) -> Result<Taken> {
        let held: Vec<&str> = held.iter().map(Uuid7::as_str).collect();
        let mut connection = self.pool().acquire().await?;
        self.tally().statements(1);
        let row: Option<PgRow> = sqlx::query(queue::CLAIM_HEAD)
            .bind(runner.as_str())
            .bind(&held)
            .bind(LEASE_TTL)
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::LEASE_STATUS_EXPIRED)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(afd_core::event::status::RECEIVED)
            .bind(afd_admission::Producer::Steer.as_str())
            .bind(self.poll_key().map(Uuid7::as_str))
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        drop(connection);
        let Some(row) = row else {
            return Ok(Taken::Nothing);
        };
        let arm = Arm::from_row(&row).map_err(query(CONTEXT))?;
        if let Some(lease_id) = arm.replayed_lease.as_deref() {
            return Ok(replayed(lease_id, &arm, runner));
        }
        let admission = match Admission::from_row(&row) {
            Ok(admission) => admission,
            Err(undecodable) => {
                self.let_go_undecoded(&arm).await;
                return Err(query(CONTEXT)(undecodable));
            }
        };
        let held = affinity::held_claim(admission.held, admission.holder.as_deref(), runner);
        // The same committed claim, unreadable one field later: freed the same
        // way, or the fleet would wait out a lease nobody holds.
        let acquired = match admission.acquired(runner) {
            Ok(acquired) => acquired,
            Err(malformed) => {
                self.let_go_undecoded(&arm).await;
                return Err(malformed);
            }
        };
        if held == Some(HeldClaim::OtherAfterLapse) {
            producers::fleet::hold::claimed(HeldClaim::OtherAfterLapse);
        }
        producers::fleet::run_started(started(acquired.kind));
        affinity::count_resumed(&acquired);
        Ok(Taken::Work(Box::new(acquired)))
    }

    /// The statement committed a claim this daemon cannot read: free the slot
    /// behind the retry backoff, so the fleet does not wait out a whole lease
    /// and does not sit at the head of every poll either.
    async fn let_go_undecoded(&self, arm: &Arm) {
        if let Ok(fleet) = Uuid7::parse(&arm.fleet_id) {
            self.let_go(&fleet, Fence::from_i64(arm.fencing_token), Backoff::Retry)
                .await;
        }
    }
}

/// The replay arm's answer: counted when it freed a lease, and logged.
fn replayed(lease_id: &str, arm: &Arm, runner: &Uuid7) -> Taken {
    let freed = arm.replay_freed;
    if freed {
        producers::fleet::lease_poll_replayed();
    }
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner_id = runner.as_str();
    let fleet_id = arm.fleet_id.as_str();
    tracing::info!(
        event = EVENT_POLL_REPLAYED,
        runner_id,
        lease_id,
        fleet_id,
        freed,
        "a worker repeated a poll whose reply it never read"
    );
    Taken::Replayed
}

impl Admission {
    /// The envelope the pass runs on, read from the committed claim.
    fn acquired(self, runner: &Uuid7) -> Result<Acquired> {
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
        let delivery = if self.opened {
            Delivery::First
        } else if self.event_status == afd_core::event::status::RECEIVED {
            Delivery::Repeat
        } else {
            Delivery::Terminal
        };
        Ok(Acquired {
            fleet_id: Uuid7::parse(&self.fleet_id).map_err(row_malformed(LEDGER, "fleet_id"))?,
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
            delivery,
            opened_at: self.opened_at,
        })
    }
}
