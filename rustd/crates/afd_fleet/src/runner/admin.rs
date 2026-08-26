//! Operator-driven administrative state changes for runners.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_wire::admin::{AdminState, RunnerAdminAction};
use sqlx::Row as _;

use crate::error::{Result, admin_state_malformed, query, rejected, runner_not_found};
use crate::runner::Runners;
use crate::sql;

const CONTEXT_TRANSITION: &str = "runner admin transition";
const DETAIL_ACTION_REQUIRED: &str = "runner action does not change administrative state";
const DETAIL_REVOKED_TERMINAL: &str =
    "revoked runners cannot transition back to cordoned or draining";

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
            row.try_get("changed"),
            target,
        )
    }

    fn admin_event_id(&self, now: UnixMillis) -> Result<Uuid7> {
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
