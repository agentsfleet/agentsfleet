//! Operator-driven administrative state changes for runners.

use afd_api_wire::admin::{AdminState, RunnerAdminAction};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{
    Result, admin_state_malformed, query, rejected, runner_not_found, runner_not_revoked,
    runner_still_leased, selftest_refused,
};
use crate::sql;
use crate::store::Runners;

const CONTEXT_TRANSITION: &str = "runner admin transition";
const CONTEXT_SELFTEST: &str = "runner self-test request";
const CONTEXT_DELETE: &str = "runner record retirement";
pub(crate) const COLUMN_ADMIN_STATE: &str = "admin_state";
const COLUMN_CHANGED: &str = "changed";
const COLUMN_LEASED: &str = "leased";
const DETAIL_ACTION_REQUIRED: &str = "runner action does not change administrative state";
const DETAIL_REVOKED_TERMINAL: &str =
    "revoked runners cannot transition back to cordoned or draining";

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
        actor_id: &str,
        now: UnixMillis,
    ) -> Result<AdminState> {
        let target = target(action).ok_or_else(|| rejected(DETAIL_ACTION_REQUIRED))?;
        let event_id = self.admin_event_id(now)?;
        let target_wire = state_wire(target);
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner_admin::TRANSITION_RUNNER_ADMIN_STATE)
            .bind(runner.as_str())
            .bind(target_wire)
            .bind(now.as_millis())
            .bind(target == AdminState::Revoked)
            .bind(event_id.as_str())
            .bind(event_wire(target))
            .bind(sql::meta::FROM_ADMIN_STATE)
            .bind(sql::meta::TO_ADMIN_STATE)
            .bind(sql::meta::ACTOR_ID)
            .bind(actor_id)
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

    /// Retires a revoked runner's record.
    ///
    /// The destructive step is the revoke; this deletes a row the revoke has
    /// already made inert, so a runner still in service is refused rather
    /// than escalated. The terminal-state check and the delete are one
    /// statement, so a concurrent revoke cannot fall between them.
    ///
    /// # Errors
    /// Returns a typed refusal for a missing runner, a runner not yet revoked,
    /// a runner still holding an active lease, or an unavailable datastore.
    pub async fn delete_revoked(&self, runner: &Uuid7) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner_admin::DELETE_RUNNER_IF_IN_STATE)
            .bind(runner.as_str())
            .bind(state_wire(AdminState::Revoked))
            .bind(sql::LEASE_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_DELETE))?
            .ok_or_else(runner_not_found)?;
        let changed: bool = row.try_get(COLUMN_CHANGED).map_err(query(CONTEXT_DELETE))?;
        let leased: bool = row.try_get(COLUMN_LEASED).map_err(query(CONTEXT_DELETE))?;
        match (changed, leased) {
            (true, _) => Ok(()),
            (false, true) => Err(runner_still_leased()),
            (false, false) => Err(runner_not_revoked()),
        }
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
        let row = sqlx::query(sql::runner_admin::PATCH_RUNNER_SELFTEST_REQUEST)
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
        Ok(self.entropy().uuid7(now)?)
    }
}

fn transition_result(
    current: Result<String, sqlx::Error>,
    changed: Result<bool, sqlx::Error>,
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

pub(crate) fn stored_state(
    raw: Result<String, sqlx::Error>,
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
        RunnerAdminAction::Rotate | RunnerAdminAction::SelfTest => None,
    }
}

/// The stored spelling of `state`, which every statement guarding on it binds.
pub(crate) const fn state_wire(state: AdminState) -> &'static str {
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
        assert_eq!(target(RunnerAdminAction::Rotate), None);
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
