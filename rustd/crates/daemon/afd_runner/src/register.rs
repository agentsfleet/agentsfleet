//! Registering a runner: the platform entry and the tenant entry share one
//! prepared insert, and a tenant registration carries a retry receipt.

use af_core::id::Uuid7;
use af_wire::runner::{AssignedPolicy, RegisterRequest};
use afd_auth::minted::Minted;
use jiff::Timestamp;
use sqlx::{Acquire as _, Postgres, Transaction};

use crate::edit::DETAIL_TENANT_DECISION_MODEL;
use crate::error::{
    Result, query, registration_key_reused, rejected, runner_not_found, undigestible,
};
use crate::policy::SandboxConfig;
use crate::reconcile::{Verdict, reconcile};
use crate::scope::Owner;
use crate::spelling::{policy_wire, render_list, tier_wire};
use crate::sql;
use crate::store::{CONTEXT_REGISTER, Enrolled, Runners, record_enrolled};
use crate::validate::registration;

/// What a tenant registration answered.
#[derive(Debug)]
pub enum Registered {
    /// A new runner, with its one-time token.
    Created(Enrolled),
    /// The runner an earlier request under the same key created. Its token was
    /// shown once and never stored, so a replay carries none.
    Replayed {
        /// The runner the first request created.
        runner_id: Uuid7,
        /// The worker count as first registered and clamped.
        worker_count: u32,
    },
}

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
    pub(crate) async fn insert(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        owner: Owner<'_>,
        actor_id: &str,
    ) -> Result<()> {
        crate::decision::lock(transaction, self.decision_model_id.as_ref())
            .await?
            .allow_change_from(None)?;
        sql::runner::RegisterRow {
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
        .map_err(query(CONTEXT_REGISTER))?;
        Ok(())
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
    /// Registers a runner for `tenant` under the request's `operation` key.
    ///
    /// The receipt and the runner commit together. A second request under the
    /// same key waits on the receipt's unique index, inserts nothing, and then
    /// reads the committed receipt: the same request replays the runner without
    /// its token, and a different one is refused. A tenant runner never holds a
    /// platform decision model, and starts with durable tasks off, which only a
    /// platform admin may switch on.
    ///
    /// # Errors
    /// Refuses a malformed request or a decision model, refuses a key reused for
    /// a different request, and reports an unavailable datastore.
    pub async fn register_tenant(
        &self,
        tenant: &Uuid7,
        operation: &Uuid7,
        request: &RegisterRequest<'_>,
        actor_id: &str,
        now: Timestamp,
    ) -> Result<Registered> {
        if request.assigned_policy.decision_model_id.is_some() {
            return Err(rejected(DETAIL_TENANT_DECISION_MODEL));
        }
        let digest = request_digest(request)?;
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
        let receipt: Option<Uuid7> = sqlx::query_scalar(sql::runner::INSERT_REGISTRATION_RECEIPT)
            .bind(self.entropy().uuid7(now)?)
            .bind(tenant)
            .bind(operation)
            .bind(&digest)
            .bind(&prepared.runner_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_REGISTER))?;
        if receipt.is_none() {
            // Roll back, then read the committed receipt on this same connection:
            // a request holds one pool connection, so concurrent retries cannot
            // exhaust the pool waiting on a second.
            drop(transaction);
            return replay(
                &mut connection,
                tenant,
                operation,
                &digest,
                prepared.worker_count,
            )
            .await;
        }
        prepared
            .insert(&mut transaction, Owner::Tenant(tenant), actor_id)
            .await?;
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_REGISTER))?;
        Ok(Registered::Created(prepared.enrolled()))
    }
}

/// Answers a key that already registered a runner.
async fn replay(
    connection: &mut sqlx::PgConnection,
    tenant: &Uuid7,
    operation: &Uuid7,
    digest: &str,
    worker_count: u32,
) -> Result<Registered> {
    let (stored_digest, runner_id): (String, Uuid7) =
        sqlx::query_as(sql::runner::SELECT_REGISTRATION_RECEIPT)
            .bind(tenant)
            .bind(operation)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_REGISTER))?
            .ok_or_else(runner_not_found)?;
    if stored_digest != digest {
        return Err(registration_key_reused());
    }
    Ok(Registered::Replayed {
        runner_id,
        worker_count,
    })
}

/// The SHA-256 of the request as parsed, so whitespace and key order in the
/// body never change it.
fn request_digest(request: &RegisterRequest<'_>) -> Result<String> {
    let body = serde_json::to_vec(request).map_err(undigestible)?;
    Ok(af_core::etag::sha256_hex(&body))
}
