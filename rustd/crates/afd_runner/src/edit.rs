//! An operator's edit of one runner: its assigned policy, its labels, or both.
//!
//! One transaction for either half or both, which is what the runner Edit
//! dialog saves: a policy the row refuses leaves the labels unwritten too, so
//! no save lands half-applied. The two halves still never touch each other's
//! columns; each has its own statement.

use std::borrow::Cow;

use afd_api_wire::admin::AdminState;
use afd_core::id::Uuid7;
use afd_wire::runner::AssignedPolicy;
use jiff::Timestamp;
use sqlx::{Acquire as _, Postgres, Transaction};

use crate::admin::stored_state;
use crate::error::{Result, query, rejected, runner_not_found};
use crate::policy::SandboxConfig;
use crate::spelling::{policy_wire, render_list, tier_wire};
use crate::store::Runners;
use crate::validate::{assignment, labels};
use crate::{policy, reconcile, sql};

/// The statement name a failed edit reports.
const CONTEXT_EDIT: &str = "runner edit";
const EVENT_POLICY_ASSIGNED: &str = "runner_policy_assigned";
const DETAIL_REVOKED_NO_POLICY: &str = "revoked runners cannot be re-assigned a policy";
const DETAIL_EDIT_EMPTY: &str = "a runner edit carries an assigned policy, labels, or both";

/// What an edit changes. At least one half is present.
#[derive(Debug, Clone, Copy)]
pub struct RunnerEdit<'r, 'a> {
    /// The policy to assign, proved and clamped before the row is read.
    pub assigned_policy: Option<&'r AssignedPolicy<'a>>,
    /// The whole label set to store, proved and collapsed by enrolment's rule.
    pub labels: Option<&'r [Cow<'a, str>]>,
}

/// What a stored edit echoes over HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerEdited<'a> {
    /// The status the runner kept; an edit never moves it.
    pub status: AdminState,
    /// The policy as stored, worker count clamped, when one was assigned.
    pub assigned_policy: Option<AssignedPolicy<'a>>,
    /// The labels as stored, each once, when labels were sent.
    pub labels: Option<Vec<Cow<'a, str>>>,
}

/// A policy proved and rendered before any connection is taken.
struct Assignment<'a> {
    stored: AssignedPolicy<'a>,
    registry_json: String,
    sandbox_config_json: String,
    event_id: Uuid7,
}

/// [`sql::runner_admin::SELECT_RUNNER_PATCH_STATE`]'s row.
#[derive(Debug, sqlx::FromRow)]
struct PatchState {
    status: String,
    capability_report: Option<String>,
}

impl Runners {
    /// Applies a policy, a label set, or both, in one transaction.
    ///
    /// The row lock covers the capability read, the policy and verdict write,
    /// its event and the label write, so a tightened assignment is never
    /// visible beside a stale healthy verdict and a refused half writes
    /// nothing. Repeating the stored policy appends no event; repeating the
    /// stored labels leaves the row byte-identical. A revoked runner's labels
    /// stay editable, since they place nothing; its policy does not. `now`
    /// mints the policy event's identifier; the database stamps the row.
    ///
    /// # Errors
    /// Refuses an empty edit, an unsafe assignment or labels past their bounds
    /// before any connection is taken; refuses a policy for a revoked runner;
    /// reports a missing runner, a malformed stored state, or an unavailable
    /// datastore.
    pub async fn edit<'a>(
        &self,
        runner: &Uuid7,
        edit: RunnerEdit<'_, 'a>,
        now: Timestamp,
    ) -> Result<RunnerEdited<'a>> {
        let labels = edit.labels.map(labels).transpose()?;
        let assignment = edit
            .assigned_policy
            .map(|requested| self.prepare(requested, now))
            .transpose()?;
        if assignment.is_none() && labels.is_none() {
            return Err(rejected(DETAIL_EDIT_EMPTY));
        }
        let labels_json = labels.as_deref().map(render_list);

        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_EDIT))?;
        let row = sqlx::query_as::<_, PatchState>(sql::runner_admin::SELECT_RUNNER_PATCH_STATE)
            .bind(runner.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_EDIT))?
            .ok_or_else(runner_not_found)?;
        let status = stored_state(&row.status)?;
        if let Some(assignment) = &assignment {
            if status == AdminState::Revoked {
                return Err(rejected(DETAIL_REVOKED_NO_POLICY));
            }
            assign(&mut transaction, runner, assignment, row.capability_report).await?;
        }
        if let Some(labels_json) = labels_json {
            sqlx::query(sql::runner_admin::REPLACE_RUNNER_LABELS)
                .bind(runner.as_str())
                .bind(labels_json)
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT_EDIT))?;
        }
        transaction.commit().await.map_err(query(CONTEXT_EDIT))?;

        Ok(RunnerEdited {
            status,
            assigned_policy: assignment.map(|assignment| assignment.stored),
            labels,
        })
    }

    /// Proves the assignment, clamps its worker count and renders its lists.
    fn prepare<'a>(
        &self,
        requested: &AssignedPolicy<'a>,
        now: Timestamp,
    ) -> Result<Assignment<'a>> {
        let validated = assignment(requested)?;
        let mut stored = requested.clone();
        stored.worker_count = validated.worker_count.get();
        Ok(Assignment {
            registry_json: render_list(&stored.registry_allowlist),
            sandbox_config_json: SandboxConfig {
                extra_binds: stored.extra_binds.clone(),
            }
            .render(),
            event_id: self.admin_event_id(now)?,
            stored,
        })
    }
}

/// Writes the assignment and its reconciled verdict inside the caller's
/// transaction, which already holds the row lock.
async fn assign(
    transaction: &mut Transaction<'_, Postgres>,
    runner: &Uuid7,
    assignment: &Assignment<'_>,
    capability_json: Option<String>,
) -> Result<()> {
    let stored = &assignment.stored;
    let capability = policy::capability(capability_json.as_deref());
    let verdict = reconcile::reconcile(stored, capability.as_ref());
    sqlx::query(sql::runner_admin::PATCH_RUNNER_ASSIGNED_POLICY)
        .bind(runner.as_str())
        .bind(tier_wire(stored.sandbox_tier))
        .bind(policy_wire(stored.network_policy))
        .bind(&assignment.registry_json)
        // `assignment` clamped this into 1..=64, so the signed form is exact
        // and matches the existing enrolment binder.
        .bind(stored.worker_count.cast_signed())
        .bind(assignment.event_id.as_str())
        .bind(EVENT_POLICY_ASSIGNED)
        .bind(sql::meta::SANDBOX_TIER)
        .bind(sql::meta::NETWORK_POLICY)
        .bind(sql::meta::REGISTRY_ALLOWLIST)
        .bind(sql::meta::WORKER_COUNT)
        .bind(verdict.reason())
        .bind(&assignment.sandbox_config_json)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(CONTEXT_EDIT))?;
    Ok(())
}
