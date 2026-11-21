//! A run's compacted context, saved through the runner's fenced context
//! verb: the daemon half of `Up::ContextSave`, for a setup draft's run and a
//! fleet chat's alike.
//!
//! The save is [`crate::context`]'s compare-and-swap on the thread's
//! `memory.context_checkpoints` row, written only while the run holds the
//! thread at its fence. A run a reassignment superseded writes nothing, and
//! its save is refused with `RUN_STALE_FENCING_TOKEN`, as its late report is.

use af_context::{Checkpoint, CheckpointStore as _};
use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::context::{Holder, Owner, Repository};
use crate::{Drafts, Result, error::raise::query};

/// Names the save's statements in a query failure.
const CONTEXT: &str = "save a run's context";

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

impl Drafts {
    /// Saves `save`'s checkpoint for the thread `save.run_id` runs, a draft's
    /// or a fleet's, as `runner_id`'s context verb asks.
    ///
    /// # Errors
    /// `RUN_STALE_FENCING_TOKEN` when the run is not `runner_id`'s live run of
    /// its thread at `save.fence`, the fence the thread still holds; nothing
    /// is written. The stale-context refusal when another save moved the row
    /// past `save.resource_version`, or the checkpoint is not the thread's
    /// next one. A store that would not answer, or an id that could not be
    /// minted.
    pub async fn save_context(
        &self,
        runner_id: &Uuid7,
        save: ContextSave,
        now: Timestamp,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let owners: Option<(Option<Uuid7>, Option<Uuid7>)> =
            sqlx::query_as(super::sql::THREAD_OF_RUN)
                .bind(&save.run_id)
                .bind(runner_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT))?;
        // A run this runner never held, or one its owner's deletion removed,
        // holds no thread, so its save is refused as a superseded run's is.
        let owner = match owners {
            Some((Some(fleet), _)) => Owner::Fleet(fleet),
            Some((None, Some(draft))) => Owner::Draft(draft),
            Some((None, None)) | None => return Err(af_context::Error::stale_fence().into()),
        };
        let id = self.mint(now)?;
        let holder = Holder {
            run: &save.run_id,
            runner: runner_id,
            fence: save.fence,
            active: afd_state::sql::RUN_STATUS_ACTIVE,
        };
        Repository::fenced(&mut connection, owner, id, holder)
            .commit(save.resource_version, save.checkpoint)
            .await?;
        Ok(())
    }
}
