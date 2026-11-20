//! Registering a runner: the platform entry and the tenant entry share one
//! prepared insert. A host registers once per owner, so a retry is refused
//! naming the runner the host already is.

use af_core::id::Uuid7;
use af_wire::runner::{AssignedPolicy, RegisterRequest};
use afd_auth::minted::Minted;
use jiff::Timestamp;
use sqlx::{Acquire as _, Postgres, Transaction};

use crate::edit::DETAIL_TENANT_DECISION_MODEL;
use crate::error::{Result, host_registered, query, rejected, runner_not_found};
use crate::policy::SandboxConfig;
use crate::reconcile::{Verdict, reconcile};
use crate::scope::Owner;
use crate::spelling::{policy_wire, render_list, tier_wire};
use crate::sql;
use crate::store::{CONTEXT_REGISTER, Enrolled, Runners, record_enrolled};
use crate::validate::registration;

/// A registration proved, rendered and minted before any connection is taken.
pub(crate) struct Prepared<'r> {
    host_id: &'r str,
    decision_model_id: Option<Uuid7>,
    assigned: AssignedPolicy<'r>,
    worker_count: u32,
    verdict: Verdict,
    labels_json: String,
    registry_json: String,
    sandbox_config_json: String,
    runner_id: Uuid7,
    event_id: Uuid7,
    token: Minted,
}

impl<'r> Prepared<'r> {
    /// Validates `request`, clamps its worker count and mints the runner's ids
    /// and token. The verdict is reconciled against no capability report, so an
    /// assignment that demands enforcement opens degraded until the first beat.
    pub(crate) fn of(
        runners: &Runners,
        request: &'r RegisterRequest<'r>,
        now: Timestamp,
    ) -> Result<Self> {
        let stored = registration(request)?;
        let worker_count = stored.assignment.worker_count.get();
        let assigned = AssignedPolicy {
            worker_count,
            ..request.assigned_policy.clone()
        };
        let (runner_id, event_id, token) = runners.mint(now)?;
        Ok(Self {
            host_id: &request.host_id,
            decision_model_id: stored.assignment.decision_model_id,
            verdict: reconcile(&assigned, None),
            labels_json: render_list(&stored.labels),
            registry_json: render_list(&assigned.registry_allowlist),
            sandbox_config_json: SandboxConfig {
                extra_binds: assigned.extra_binds.clone(),
            }
            .render(),
            assigned,
            worker_count,
            runner_id,
            event_id,
            token,
        })
    }

    /// Writes the runner and its registration event, naming `actor_id`,
    /// inside `transaction`.
    ///
    /// # Errors
    /// Refuses a host `owner` already registered, naming that runner. The
    /// insert skips it rather than failing on the unique index, so the read
    /// that names it runs in the same transaction; a concurrent registration
    /// of the host waits on the index and then reads the committed runner.
    pub(crate) async fn insert(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        owner: Owner<'_>,
        actor_id: &str,
    ) -> Result<()> {
        crate::decision::lock(transaction, self.decision_model_id.as_ref())
            .await?
            .allow_change_from(None)?;
        let inserted = sql::runner::RegisterRow {
            durable_tasks_enabled: self.assigned.durable_tasks_enabled,
            sandbox_config_json: &self.sandbox_config_json,
            decision_model_id: self.decision_model_id.as_ref(),
            runner_id: &self.runner_id,
            host_id: self.host_id,
            token_digest: self.token.digest().as_str(),
            sandbox_tier: tier_wire(self.assigned.sandbox_tier),
            status: sql::RUNNER_STATUS_ACTIVE,
            labels_json: &self.labels_json,
            last_seen_at: sql::NEVER_SEEN,
            event_id: &self.event_id,
            event_type: sql::event_type::RUNNER_REGISTERED,
            tenant_id: owner.tenant(),
            platform_owned: owner.platform(),
            network_policy: policy_wire(self.assigned.network_policy),
            registry_allowlist_json: &self.registry_json,
            worker_count: self.worker_count.cast_signed(),
            degraded_reason: self.verdict.reason(),
            actor_id,
        }
        .bind()
        .execute(&mut **transaction)
        .await
        .map_err(query(CONTEXT_REGISTER))?
        .rows_affected();
        if inserted > 0 {
            return Ok(());
        }
        let registered: Uuid7 = sqlx::query_scalar(sql::runner::SELECT_HOST_RUNNER)
            .bind(owner.tenant())
            .bind(self.host_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_REGISTER))?
            .ok_or_else(runner_not_found)?;
        Err(host_registered(registered))
    }

    /// The created runner, its token revealed to this one caller.
    pub(crate) fn enrolled(self) -> Enrolled {
        record_enrolled(&self.runner_id, self.host_id, self.verdict);
        Enrolled {
            runner_id: self.runner_id,
            token: self.token,
            worker_count: self.worker_count,
            verdict: self.verdict,
        }
    }
}

impl Runners {
    /// Registers a runner for `tenant`.
    ///
    /// A host registers once: a second registration of the same `host_id`
    /// writes nothing and is refused naming the runner the host already is,
    /// whose token the tenant rotates to recover it. A tenant runner never
    /// holds a platform decision model, and starts with durable tasks off,
    /// which only a platform admin may switch on.
    ///
    /// # Errors
    /// Refuses a malformed request or a decision model, refuses a host the
    /// tenant already registered, and reports an unavailable datastore.
    pub async fn register_tenant(
        &self,
        tenant: &Uuid7,
        request: &RegisterRequest<'_>,
        actor_id: &str,
        now: Timestamp,
    ) -> Result<Enrolled> {
        if request.assigned_policy.decision_model_id.is_some() {
            return Err(rejected(DETAIL_TENANT_DECISION_MODEL));
        }
        let tasks_off = RegisterRequest {
            assigned_policy: AssignedPolicy {
                durable_tasks_enabled: false,
                ..request.assigned_policy.clone()
            },
            ..request.clone()
        };
        let prepared = Prepared::of(self, &tasks_off, now)?;
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_REGISTER))?;
        prepared
            .insert(&mut transaction, Owner::Tenant(tenant), actor_id)
            .await?;
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_REGISTER))?;
        Ok(prepared.enrolled())
    }
}
