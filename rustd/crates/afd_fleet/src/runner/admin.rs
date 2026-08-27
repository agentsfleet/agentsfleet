//! Operator-driven administrative state changes for runners.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_wire::admin::{AdminState, RunnerAdminAction};
use afd_wire::runner::AssignedPolicy;
use sqlx::{Acquire as _, Row as _};

use crate::error::{
    Result, admin_state_malformed, query, rejected, runner_not_found, selftest_refused,
};
use crate::runner::Runners;
use crate::sql;

const CONTEXT_TRANSITION: &str = "runner admin transition";
const CONTEXT_POLICY: &str = "runner policy assignment";
const CONTEXT_SELFTEST: &str = "runner self-test request";
const COLUMN_ADMIN_STATE: &str = "admin_state";
const COLUMN_CHANGED: &str = "changed";
const DETAIL_ACTION_REQUIRED: &str = "runner action does not change administrative state";
const DETAIL_REVOKED_TERMINAL: &str =
    "revoked runners cannot transition back to cordoned or draining";
const DETAIL_REVOKED_NO_POLICY: &str = "revoked runners cannot be re-assigned a policy";

/// What a stored policy mutation needs to echo over HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyAssigned {
    /// The state retained by the runner.
    admin_state: AdminState,
    /// The worker count after applying the shared bounds.
    worker_count: u32,
}

impl PolicyAssigned {
    /// The runner state retained by the mutation.
    #[must_use]
    pub const fn admin_state(self) -> AdminState {
        self.admin_state
    }

    /// The bounded count stored on the row.
    #[must_use]
    pub const fn worker_count(self) -> u32 {
        self.worker_count
    }
}

/// What a recorded self-test ask needs to echo over HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelftestRequested {
    /// The state retained by the runner.
    admin_state: AdminState,
    /// The request instant stored on the row.
    requested_at: i64,
}

impl SelftestRequested {
    /// The runner state retained by the request.
    #[must_use]
    pub const fn admin_state(self) -> AdminState {
        self.admin_state
    }

    /// The request instant stored on the row.
    #[must_use]
    pub const fn requested_at(self) -> i64 {
        self.requested_at
    }
}

impl Runners {
    /// Applies an operator action and appends the corresponding event.
    ///
    /// Repeating the current target is successful and writes nothing. A
    /// revoked runner may only be revoked again; the terminal guard is inside
    /// the same locked statement as the write, so concurrent requests cannot
    /// reopen it.
    /// # Errors
    /// Returns a typed refusal for a missing runner, a malformed stored state,
    /// an illegal transition, or an unavailable datastore.
    pub async fn transition(
        &self,
        runner: &Uuid7,
        action: RunnerAdminAction,
        now: UnixMillis,
    ) -> Result<AdminState> {
        let target = target(action).ok_or_else(|| rejected(DETAIL_ACTION_REQUIRED))?;
        let event_id = self.admin_event_id(now)?;
        let target_wire = state_wire(target);
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner::TRANSITION_RUNNER_ADMIN_STATE)
            .bind(runner.as_str())
            .bind(target_wire)
            .bind(now.as_millis())
            .bind(target == AdminState::Revoked)
            .bind(event_id.as_str())
            .bind(event_wire(target))
            .bind(sql::meta::FROM_ADMIN_STATE)
            .bind(sql::meta::TO_ADMIN_STATE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TRANSITION))?
            .ok_or_else(runner_not_found)?;
        transition_result(
            row.try_get("from_admin_state"),
            row.try_get(COLUMN_CHANGED),
            target,
        )
    }

    /// Replaces a runner's assignment and reconciles its placement verdict in
    /// the same transaction.
    ///
    /// Repeating the stored values is successful and appends no event. The row
    /// lock covers the capability read, policy write, verdict write, and event,
    /// so a tightened assignment is never visible beside a stale healthy
    /// verdict.
    ///
    /// # Errors
    /// Refuses an unsafe assignment, a missing runner, a revoked runner, a
    /// malformed stored state, or an unavailable datastore.
    pub async fn assign_policy(
        &self,
        runner: &Uuid7,
        requested: &AssignedPolicy<'_>,
        now: UnixMillis,
    ) -> Result<PolicyAssigned> {
        let validated = super::validate::assignment(requested)?;
        let mut stored = requested.clone();
        stored.worker_count = validated.worker_count.get();
        let registry_json = super::spelling::render_list(&stored.registry_allowlist);
        let extra_binds_json = super::spelling::render_list(&stored.extra_binds);
        let event_id = self.admin_event_id(now)?;

        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_POLICY))?;
        let row = sqlx::query(sql::runner::SELECT_RUNNER_PATCH_STATE)
            .bind(runner.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_POLICY))?
            .ok_or_else(runner_not_found)?;
        let state = stored_state(row.try_get(COLUMN_ADMIN_STATE), CONTEXT_POLICY)?;
        if state == AdminState::Revoked {
            return Err(rejected(DETAIL_REVOKED_NO_POLICY));
        }
        let capability_json: Option<String> = row
            .try_get("capability_report")
            .map_err(query(CONTEXT_POLICY))?;
        let capability = super::policy::capability(capability_json.as_deref());
        let verdict = super::reconcile::reconcile(Some(&stored), capability.as_ref());

        sqlx::query(sql::runner::PATCH_RUNNER_ASSIGNED_POLICY)
            .bind(runner.as_str())
            .bind(super::spelling::tier_wire(stored.sandbox_tier))
            .bind(super::spelling::policy_wire(stored.network_policy))
            .bind(registry_json)
            // `assignment` clamped this into 1..=64, so the signed form is
            // exact and matches the existing enrolment binder.
            .bind(stored.worker_count.cast_signed())
            .bind(now.as_millis())
            .bind(event_id.as_str())
            .bind("runner_policy_assigned")
            .bind(sql::meta::SANDBOX_TIER)
            .bind(sql::meta::NETWORK_POLICY)
            .bind(sql::meta::REGISTRY_ALLOWLIST)
            .bind(sql::meta::WORKER_COUNT)
            .bind(verdict.is_degraded())
            .bind(verdict.reason())
            .bind(extra_binds_json)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_POLICY))?;
        transaction.commit().await.map_err(query(CONTEXT_POLICY))?;

        Ok(PolicyAssigned {
            admin_state: state,
            worker_count: stored.worker_count,
        })
    }

    /// Records a self-test request for collection on the next heartbeat.
    ///
    /// # Errors
    /// Returns the dedicated conflict for a revoked runner, not-found for a
    /// missing runner, or the canonical datastore failures.
    pub async fn request_selftest(
        &self,
        runner: &Uuid7,
        now: UnixMillis,
    ) -> Result<SelftestRequested> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner::PATCH_RUNNER_SELFTEST_REQUEST)
            .bind(runner.as_str())
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SELFTEST))?
            .ok_or_else(runner_not_found)?;
        let state = stored_state(row.try_get(COLUMN_ADMIN_STATE), CONTEXT_SELFTEST)?;
        let changed: bool = row
            .try_get(COLUMN_CHANGED)
            .map_err(query(CONTEXT_SELFTEST))?;
        if !changed {
            return Err(selftest_refused());
        }
        Ok(SelftestRequested {
            admin_state: state,
            requested_at: now.as_millis(),
        })
    }

    pub(super) fn admin_event_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        Uuid7::encode(now, bytes).map_err(Into::into)
    }
}

fn transition_result(
    current: core::result::Result<String, sqlx::Error>,
    changed: core::result::Result<bool, sqlx::Error>,
    target: AdminState,
) -> Result<AdminState> {
    let current = current.map_err(query(CONTEXT_TRANSITION))?;
    let changed = changed.map_err(query(CONTEXT_TRANSITION))?;
    let state: AdminState =
        afd_core::spelling::from_spelling(&current).ok_or_else(admin_state_malformed)?;
    if !changed && state == AdminState::Revoked && target != AdminState::Revoked {
        return Err(rejected(DETAIL_REVOKED_TERMINAL));
    }
    Ok(target)
}

fn stored_state(
    raw: core::result::Result<String, sqlx::Error>,
    context: &'static str,
) -> Result<AdminState> {
    let raw = raw.map_err(query(context))?;
    afd_core::spelling::from_spelling(&raw).ok_or_else(admin_state_malformed)
}

/// The target state for a mutation action; self-test is not a transition.
#[must_use]
pub const fn target(action: RunnerAdminAction) -> Option<AdminState> {
    match action {
        RunnerAdminAction::Cordon => Some(AdminState::Cordoned),
        RunnerAdminAction::Drain => Some(AdminState::Draining),
        RunnerAdminAction::Revoke => Some(AdminState::Revoked),
        RunnerAdminAction::SelfTest => None,
    }
}

const fn state_wire(state: AdminState) -> &'static str {
    match state {
        AdminState::Active => "active",
        AdminState::Cordoned => "cordoned",
        AdminState::Draining => "draining",
        AdminState::Drained => "drained",
        AdminState::Revoked => "revoked",
    }
}

const fn event_wire(state: AdminState) -> &'static str {
    match state {
        AdminState::Active => "runner_online",
        AdminState::Cordoned => "runner_cordoned",
        AdminState::Draining => "runner_draining",
        AdminState::Drained => "runner_drained",
        AdminState::Revoked => "runner_revoked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_name_only_real_state_transitions() {
        assert_eq!(
            target(RunnerAdminAction::Cordon),
            Some(AdminState::Cordoned)
        );
        assert_eq!(target(RunnerAdminAction::Drain), Some(AdminState::Draining));
        assert_eq!(target(RunnerAdminAction::Revoke), Some(AdminState::Revoked));
        assert_eq!(target(RunnerAdminAction::SelfTest), None);
    }

    #[test]
    fn stored_spellings_round_trip_through_the_wire_enum() {
        for state in [
            AdminState::Active,
            AdminState::Cordoned,
            AdminState::Draining,
            AdminState::Drained,
            AdminState::Revoked,
        ] {
            assert_eq!(
                afd_core::spelling::from_spelling(state_wire(state)),
                Some(state)
            );
        }
    }
}
