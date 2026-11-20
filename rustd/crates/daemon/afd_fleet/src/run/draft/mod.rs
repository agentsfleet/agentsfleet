//! A setup draft's turn on the common run path: assignment, renewal,
//! report, each on the shared run statements ([`super::sql::shared`]).
//!
//! A draft's run is a fleet-like run the draft owns
//! ([`super::RunOwner::Draft`]): the draft's thread (`thread.threads.id` is the
//! draft's id) fences it, its `turn_id` is the draft message it runs, and the
//! runner reaches the daemon only through the same fenced verbs a fleet run
//! uses. The draft's own rows (which message, its finish, the one candidate)
//! are `afd_setup::setup_run`'s, written in these transactions.
//!
//! What differs from a fleet's run:
//! - No fleet gates and no fleet budget: the payer's balance is checked, and
//!   nothing is reserved.
//! - Key custody: a tenant's runner is never assigned a draft whose message
//!   runs on the platform model, whose key the run carries.
//! - A short `max_runtime_seconds`, the setup turn's wall, so one runaway
//!   costs minutes.
//! - No sandbox and one tool, `propose_setup`; no turn checkpoint. A lapsed
//!   run is reassigned and reruns its message from the start, at most
//!   [`af_core::limits::EVENT_RUNS_MAX`] runs, then the message fails.
//! - The draft's compacted context, read through the fenced context verb
//!   (`context`) and saved through `afd_setup`'s fenced save.
//! - Assigned only to a protocol v4 runner; a v2 runner keeps the authoring
//!   attempt path (`afd_setup::run`) until the runner's setup agent
//!   replaces it.

mod assign;
mod context;
mod renew;
mod report;

use af_core::id::Uuid7;

pub use assign::DraftAssignment;
pub use context::DraftContext;

/// What a session keeps about a draft-owned run it assigned: the draft, so
/// its renewals, report and live answer take the draft's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTarget {
    /// The draft the run runs a turn of.
    pub draft_id: Uuid7,
}

/// [`super::sql::draft::SELECT_DRAFT_RUN`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct DraftRun {
    draft_id: Uuid7,
    turn_id: Uuid7,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    posture: String,
    provider: String,
    model: String,
    fencing_token: i64,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
    cancelled: bool,
}

impl super::Runs {
    /// The draft-owned run `run` names, if `runner` holds it.
    async fn draft_run(&self, run: &Uuid7, runner: &Uuid7) -> crate::Result<Option<DraftRun>> {
        sqlx::query_as(super::sql::draft::SELECT_DRAFT_RUN)
            .bind(run)
            .bind(runner)
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(crate::error::query("read a draft run"))
    }
}
