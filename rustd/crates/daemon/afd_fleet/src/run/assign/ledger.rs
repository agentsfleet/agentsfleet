//! Claim a slot and its exact oldest unsettled turn in one transaction.

use af_core::id::Uuid7;

use af_observability::metrics::label::fleet::HeldClaim;
use af_observability::producers;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Connection as _;

use super::{MAX_CANDIDATES_PER_POLL, PollCost, started};
use crate::error::{Result, query};
use crate::run::affinity::{self, Fence};
use crate::run::envelope::{Acquired, HeadKind, Kind, Opened};
use crate::run::event::{Delivery, Received};
use crate::run::installed::FleetWithSession;
use crate::run::offer::{RecoveryKey, RecoveryScan};
use crate::run::sql::{self, queue};
use crate::run::store::Runs;
use afd_credential::provider::cache::PlatformKeyKey;

/// Query context shared by the claim's transaction operations.
const CONTEXT: &str = "admissions ledger claim";

/// The admission row whose slot and fence the transaction just won.
#[derive(Debug, sqlx::FromRow)]
struct Admission {
    predecessor_run_id: Option<Uuid7>,
    turn_id: Uuid7,
    workspace_id: Uuid7,
    actor: String,
    event_type: String,
    request_json: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    turn_created_at: Timestamp,
    redelivered: bool,
    fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    run_expires_at: Timestamp,
    held: bool,
    holder: Option<Uuid7>,
    reclaimed: bool,
    earlier_runs: i64,
    tenant_id: Uuid7,
    #[sqlx(flatten)]
    memory: MemoryScope,
    #[sqlx(flatten)]
    setup: FleetWithSession,
    #[sqlx(flatten)]
    opened: OpenedRow,
}

/// The fleet's memory flags, as the claim read them.
#[derive(Debug, sqlx::FromRow)]
struct MemoryScope {
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
}

/// The event the claim opened or met on the turn, and the fleet's counters.
#[derive(Debug, sqlx::FromRow)]
struct OpenedRow {
    inserted: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    opened_at: Timestamp,
    event_status: String,
    event_count: i64,
    budget_used_nanos: i64,
    catalogue_version: i64,
    platform_secret_id: Option<Uuid7>,
    platform_secret_digest: Option<String>,
    inputs_waiting: bool,
    tasked: bool,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    fleet_id: Uuid7,
    turn_id: Uuid7,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    turn_created_at: Timestamp,
}

impl Runs {
    /// A bounded candidate scan followed by atomic slot-and-head claims.
    pub(super) async fn ledger_counted(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        recovery: Option<&mut RecoveryScan>,
        cost: &mut PollCost,
    ) -> Result<Option<Acquired>> {
        let hint = if recovery.is_some() {
            None
        } else {
            self.wake().take()
        };
        let held: Vec<&Uuid7> = held.iter().collect();
        let mut connection = self.pool().acquire().await?;
        let after = recovery.as_ref().and_then(|scan| scan.after.as_ref());
        let candidates: Vec<Candidate> = sqlx::query_as(queue::SELECT_CANDIDATES)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(runner)
            .bind(i64::try_from(MAX_CANDIDATES_PER_POLL).unwrap_or(i64::MAX))
            .bind(self.clock().offline_after())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(&held)
            .bind(hint.as_ref())
            .bind(af_wire::task::CAPABILITY)
            .bind(af_wire::task::TaskState::Ready.as_str())
            .bind(af_wire::task::TaskState::Running.as_str())
            .bind(crate::task::DECISION_POLICY_PATH)
            .bind(recovery.is_some())
            .bind(after.map(|key| key.created.to_sqlx()))
            .bind(after.map(|key| &key.admission))
            .bind(self.checkpoint_resume_enabled())
            .bind(afd_credential::provider::PLATFORM_MODEL_REF)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        cost.candidates_scanned = u64::try_from(candidates.len()).unwrap_or(u64::MAX);
        for candidate in &candidates {
            if let Some(acquired) = self
                .ledger_claim(&mut connection, &candidate.fleet_id, runner)
                .await?
            {
                if let Some(scan) = recovery {
                    record_claim(scan);
                }
                producers::fleet::run_started(started(acquired.kind));
                affinity::count_resumed(&acquired);
                return Ok(Some(acquired));
            }
        }
        if let Some(scan) = recovery {
            record_page_end(scan, candidates);
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
            .bind(self.clock().ttl())
            .bind(self.clock().offline_after())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(sql::RUN_STATUS_EXPIRED)
            .bind(af_wire::task::CAPABILITY)
            .bind(af_wire::task::TaskState::Ready.as_str())
            .bind(af_wire::task::TaskState::Running.as_str())
            .bind(crate::task::DECISION_POLICY_PATH)
            .bind(sql::FLEET_STATUS_ACTIVE)
            .bind(af_core::turn::status::RECEIVED)
            .bind(self.checkpoint_resume_enabled())
            .bind(afd_credential::provider::PLATFORM_MODEL_REF)
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

fn record_claim(scan: &mut RecoveryScan) {
    if scan.after.is_some() {
        scan.claims_after_skip += 1;
        if scan.claims_after_skip >= MAX_CANDIDATES_PER_POLL {
            *scan = RecoveryScan::default();
        }
    }
}

fn record_page_end(scan: &mut RecoveryScan, mut candidates: Vec<Candidate>) {
    if candidates.len() == MAX_CANDIDATES_PER_POLL
        && let Some(last) = candidates.pop()
    {
        scan.after = Some(RecoveryKey {
            created: last.turn_created_at,
            admission: last.turn_id,
        });
        scan.deferred = true;
    } else {
        *scan = RecoveryScan::default();
    }
}

impl Admission {
    /// Build the same downstream envelope from the committed ledger payload.
    fn acquired(self, fleet: &Uuid7, runner: &Uuid7) -> Acquired {
        // The outer join finds a prior run only when the admission has been
        // issued before.
        let kind = if self.reclaimed {
            Kind::Reclaim
        } else {
            Kind::Fresh
        };
        // A false `inserted` is a turn an earlier writer already opened: its
        // stored status says whether it is still running.
        let delivery = self.opened.delivery();
        let OpenedRow {
            inserted,
            opened_at,
            event_count,
            budget_used_nanos,
            catalogue_version,
            platform_secret_id,
            platform_secret_digest,
            inputs_waiting,
            tasked,
            ..
        } = self.opened;
        let platform_key = platform_secret_id
            .zip(platform_secret_digest)
            .map(|(secret, digest)| PlatformKeyKey::observed(catalogue_version, secret, digest));
        let opened = Opened {
            received: Received {
                delivery,
                // The snapshot plus this statement's own insert, which the
                // counting trigger's write cannot show to its own statement.
                counters: inserted.then_some(afd_api_wire::tail::FleetCounters {
                    events_processed: event_count,
                    budget_used_nanos,
                }),
                opened_at,
            },
            tenant_id: self.tenant_id,
            catalogue_version,
            platform_key,
            memory_reads_workspace: self.memory.memory_reads_workspace,
            memory_publishes_workspace: self.memory.memory_publishes_workspace,
            inputs_waiting,
            head: if tasked {
                HeadKind::Task
            } else {
                HeadKind::Plain
            },
            setup: self.setup,
        };
        Acquired {
            predecessor_run_id: self.predecessor_run_id,
            fleet_id: fleet.clone(),
            fence: Fence::from_i64(self.fencing_token),
            held_until: self.run_expires_at,
            kind,
            turn_id: self.turn_id,
            actor: self.actor,
            event_type: self.event_type,
            request_json: self.request_json,
            workspace_id: self.workspace_id.clone(),
            turn_created_at: self.turn_created_at,
            resume_hold: self.held && self.holder.as_ref() == Some(runner) && kind == Kind::Fresh,
            redelivered: self.redelivered,
            earlier_runs: self.earlier_runs,
            opened,
        }
    }
}

impl OpenedRow {
    fn delivery(&self) -> Delivery {
        if self.inserted {
            Delivery::First
        } else if self.event_status == af_core::turn::status::RECEIVED {
            Delivery::Repeat
        } else {
            Delivery::Terminal
        }
    }
}
