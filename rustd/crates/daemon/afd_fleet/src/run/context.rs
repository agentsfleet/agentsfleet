//! A run's read and save of its fleet chat's compacted context: the daemon
//! half of `Up::ContextLoad` and `Up::ContextSave`.
//!
//! ```text
//!  ContextLoad { run, fencing_token } ─► this runner's run, live at that fence?
//!     no  ─► run not found, or the stale-fence refusal: nothing is read
//!     yes ─► the thread's `thread.compactions` row, validated, and its scope
//!  ContextSave { run, fencing_token, resource_version, checkpoint }
//!         ─► `afd_setup::context`'s fenced compare-and-swap on that row
//! ```
//!
//! The save proves the fence in its statement. A read changes nothing, so a
//! check before it is enough: a run superseded after the check reads a
//! context it can no longer save over.

use af_context::{Checkpoint, CheckpointStore as _};
use af_core::id::Uuid7;
use af_wire::context::ContextLoadRequest;
use afd_setup::context::{Holder, Repository, fleet_identity};
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
    /// The fleet the run's thread belongs to.
    fleet_id: Uuid7,
    fencing_token: i64,
    status: String,
}

impl RunService {
    /// The compacted context of the fleet thread `run` runs, read for
    /// `runner` while the run is live at `request`'s fence.
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
        let scope = fleet_identity(&held.fleet_id);
        // The id names a row only a first save inserts; a read never uses it.
        let unused = self.runs.entropy().uuid7(now)?;
        let checkpoint = Repository::new(&mut connection, held.fleet_id, unused)
            .load()
            .await?;
        Ok(ThreadContext { scope, checkpoint })
    }

    /// Saves `save`'s checkpoint for the fleet thread `save.run_id` runs, as
    /// `runner`'s context verb asks.
    ///
    /// # Errors
    /// The stale-fence refusal when the run is not `runner`'s live run of its
    /// thread at `save.fence`, and nothing is written. The stale-context
    /// refusal when another save moved the row past `save.resource_version`,
    /// or the checkpoint is not the thread's next one. A store that would not
    /// answer, or an id that could not be minted.
    pub async fn save_context(
        &self,
        runner: &Uuid7,
        save: ContextSave,
        now: Timestamp,
    ) -> Result<()> {
        let mut connection = self.runs.pool().acquire().await?;
        let held: Option<RunThread> = sqlx::query_as(sql::context::SELECT_RUN_THREAD)
            .bind(&save.run_id)
            .bind(runner)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("read a run's thread"))?;
        // A run this runner never held, or one its fleet's deletion removed,
        // holds no thread, so its save is refused as a superseded run's is.
        let Some(held) = held else {
            return Err(stale_fence());
        };
        let id = self.runs.entropy().uuid7(now)?;
        let holder = Holder {
            run: &save.run_id,
            runner,
            fence: save.fence,
            active: sql::RUN_STATUS_ACTIVE,
        };
        Repository::fenced(&mut connection, held.fleet_id, id, holder)
            .commit(save.resource_version, save.checkpoint)
            .await?;
        Ok(())
    }
}

/// One save a run asks for, as the context verb carries it.
#[derive(Debug)]
pub struct ContextSave {
    /// The run saving.
    pub run_id: Uuid7,
    /// The fencing token the run holds its thread at.
    pub fence: u64,
    /// The generation the save expects the stored row to hold: 0 before the
    /// thread's first save.
    pub resource_version: u64,
    /// The compacted context, whose generation is one past
    /// `resource_version`.
    pub checkpoint: Checkpoint,
}
