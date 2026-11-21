//! A run's read of its thread's compacted context: the daemon half of
//! `Up::ContextLoad`, for a setup draft's run and a fleet chat's alike.
//!
//! ```text
//!  ContextLoad { run, fencing_token } ─► this runner's run, live at that fence?
//!     no  ─► run not found, or the stale-fence refusal: nothing is read
//!     yes ─► the thread's `memory.context_checkpoints` row, validated, and its scope
//! ```
//!
//! The save is `afd_setup`'s (`Drafts::save_context`), which proves the fence
//! in its statement. A read changes nothing, so a check before it is enough:
//! a run superseded after the check reads a context it can no longer save
//! over.

use af_context::Checkpoint;
use af_core::id::Uuid7;
use af_wire::context::ContextLoadRequest;
use afd_setup::context::{Owner, Repository};
use jiff::Timestamp;

use crate::error::{Result, query, run_not_found, stale_fence};
use crate::run::{RunService, sql};

/// A thread's compacted context, as a run starts from it.
#[derive(Clone)]
pub struct ThreadContext {
    /// The scope every checkpoint of the thread names.
    pub scope: String,
    /// The stored checkpoint, absent before the thread's first save.
    pub checkpoint: Option<Checkpoint>,
}

/// Hides the notes: they are the conversation's private working state.
impl std::fmt::Debug for ThreadContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadContext")
            .field("checkpoint", &self.checkpoint.is_some())
            .finish_non_exhaustive()
    }
}

/// [`sql::context::SELECT_RUN_THREAD`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct RunThread {
    fleet_id: Option<Uuid7>,
    draft_id: Option<Uuid7>,
    fencing_token: i64,
    status: String,
}

impl RunService {
    /// The compacted context of the thread `run` runs, a draft's or a
    /// fleet's, read for `runner` while the run is live at `request`'s fence.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's, and one that is no longer
    /// live at the fence it names, reading nothing. Reports a stored context
    /// that is not a valid checkpoint, and a datastore that would not answer.
    pub async fn load_context(
        &self,
        runner: &Uuid7,
        run: &Uuid7,
        request: &ContextLoadRequest,
        now: Timestamp,
    ) -> Result<ThreadContext> {
        let mut connection = self.runs.pool().acquire().await?;
        let held: Option<RunThread> = sqlx::query_as(sql::context::SELECT_RUN_THREAD)
            .bind(run)
            .bind(runner)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("read a run's thread"))?;
        let Some(held) = held else {
            return Err(run_not_found());
        };
        if held.status != sql::RUN_STATUS_ACTIVE
            || i64::try_from(request.fencing_token).ok() != Some(held.fencing_token)
        {
            return Err(stale_fence());
        }
        let owner = match (held.fleet_id, held.draft_id) {
            (Some(fleet), _) => Owner::Fleet(fleet),
            (None, Some(draft)) => Owner::Draft(draft),
            (None, None) => return Err(run_not_found()),
        };
        let scope = owner.scope();
        // The id names a row only a first save inserts; a read never uses it.
        let unused = self.runs.entropy().uuid7(now)?;
        let checkpoint = Repository::new(&mut connection, owner, unused)
            .load()
            .await?;
        Ok(ThreadContext { scope, checkpoint })
    }
}
