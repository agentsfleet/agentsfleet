//! One probe per offer pass: whether any work waits, and which connected
//! tenant runners have none of it to take.

use af_core::id::Uuid7;
use afd_setup::types::{DraftState, MessageState};

use super::sql;
use crate::Result;
use crate::error::query;

const CONTEXT_WAITING: &str = "waiting work probe";

/// What the probe found for one pass.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Waiting {
    pending: bool,
    idle: Vec<Uuid7>,
}

impl Waiting {
    /// Whether a claim pass may help; a false positive costs one bounded pass.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// The probed tenant runners whose tenant has nothing waiting: a claim on
    /// one of them cannot run, so the pass skips it.
    #[must_use]
    pub fn idle(&self) -> &[Uuid7] {
        &self.idle
    }
}

/// Reads waiting fleet and setup work once, naming the idle tenant runners
/// among `runners`. An empty `runners` asks only whether anything waits.
///
/// # Errors
/// Reports the datastore's failure.
pub async fn waiting(
    connection: &mut sqlx::PgConnection,
    runners: &[Uuid7],
    resume_from_checkpoint: bool,
) -> Result<Waiting> {
    sqlx::query_as::<_, Waiting>(sql::queue::SELECT_WAITING_WORK)
        .bind(sql::RUN_STATUS_ACTIVE)
        .bind(DraftState::Editing.as_str())
        .bind(MessageState::Queued.as_str())
        .bind(runners)
        .bind(resume_from_checkpoint)
        .bind(MessageState::Running.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT_WAITING))
}
