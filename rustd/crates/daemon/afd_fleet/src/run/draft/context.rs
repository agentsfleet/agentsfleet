//! A setup run's read of its draft's compacted context: the daemon half of
//! `Up::ContextLoad`.
//!
//! ```text
//!  ContextLoad { run, fencing_token } ─► this runner's draft run, live at that fence?
//!     no  ─► run not found, or the stale-fence refusal: nothing is read
//!     yes ─► the draft's `memory.context_checkpoints` row, validated, and its scope
//! ```
//!
//! The save is `afd_setup`'s (`Drafts::save_context`), which proves the fence
//! in its statement. A read changes nothing, so a check before it is enough:
//! a run superseded after the check reads a context it can no longer save
//! over.

use af_context::Checkpoint;
use af_context::repository::{Repository, identity};
use af_core::id::Uuid7;
use af_wire::context::ContextLoadRequest;
use jiff::Timestamp;

use crate::error::{Result, run_not_found, stale_fence};
use crate::run::{RunService, sql};

/// A draft's compacted context, as its setup run starts from it.
#[derive(Clone)]
pub struct DraftContext {
    /// The scope every checkpoint of the draft names.
    pub scope: String,
    /// The stored checkpoint, absent before the draft's first save.
    pub checkpoint: Option<Checkpoint>,
}

/// Hides the notes: they are the draft creator's private conversation.
impl std::fmt::Debug for DraftContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DraftContext")
            .field("checkpoint", &self.checkpoint.is_some())
            .finish_non_exhaustive()
    }
}

impl RunService {
    /// The compacted context of the draft `run` runs, read for `runner` while
    /// the run is live at `request`'s fence.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's draft run, and one that is no
    /// longer live at the fence it names, reading nothing. Reports a stored
    /// context that is not a valid checkpoint, and a datastore that would not
    /// answer.
    pub async fn load_draft_context(
        &self,
        runner: &Uuid7,
        run: &Uuid7,
        request: &ContextLoadRequest,
        now: Timestamp,
    ) -> Result<DraftContext> {
        let Some(held) = self.runs.draft_run(run, runner).await? else {
            return Err(run_not_found());
        };
        if held.status != sql::RUN_STATUS_ACTIVE
            || i64::try_from(request.fencing_token).ok() != Some(held.fencing_token)
        {
            return Err(stale_fence());
        }
        // The id names a row only a first save inserts; a read never uses it.
        let unused = self.runs.entropy().uuid7(now)?;
        let mut connection = self.runs.pool().acquire().await?;
        let checkpoint = Repository::new(&mut connection, held.draft_id.clone(), unused)
            .load()
            .await?;
        Ok(DraftContext {
            scope: identity(&held.draft_id),
            checkpoint,
        })
    }
}
