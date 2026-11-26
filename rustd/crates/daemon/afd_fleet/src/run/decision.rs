//! Freeze the locked host's reviewed selection through the shared runner resolver.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use crate::Result;
use crate::run::issue::Refused;
use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::decision::DecisionAssignment;
use sqlx::PgConnection;

/// The event an unreadable stored decision profile is logged under.
const EVENT_PROFILE_UNREADABLE: &str = "runner_decision_profile_unreadable";

/// Caller keeps the host lock through writing both run and snapshot.
///
/// A stored profile this build cannot read refuses the issue as
/// [`Refused::Profile`], a stop for this runner rather than a fault that would
/// end its stream; it is logged once per revision in `logged`.
pub(super) async fn freeze(
    connection: &mut PgConnection,
    runner: &Uuid7,
    decision_selected: bool,
    logged: &Mutex<HashSet<Uuid7>>,
) -> Result<core::result::Result<Option<Box<DecisionAssignment>>, Refused>> {
    if !decision_selected {
        return Ok(Ok(None));
    }
    Ok(
        match afd_runner::decision_assignment_on(connection, runner).await? {
            None => Ok(None),
            Some(Ok(selected)) => Ok(Some(selected.assignment)),
            Some(Err(unreadable)) => {
                let fresh = logged
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(unreadable.revision_id.clone());
                if fresh {
                    unread(runner, &unreadable.revision_id);
                }
                Err(Refused::Profile)
            }
        },
    )
}

/// Logs a stored profile this build cannot read; a reviewed revision replaces it.
fn unread(runner: &Uuid7, revision: &Uuid7) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let (runner_id, revision_id) = (runner.as_str(), revision.as_str());
    {
        let (error_code, event, message) = (
            &code,
            &EVENT_PROFILE_UNREADABLE,
            format_args!("the selected decision profile does not read; its runner takes no run"),
        );
        tracing::warn!(message, error_code, runner_id, revision_id, event);
    };
}
