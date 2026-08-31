//! Bringing the external scheduler in line with what the row says.
//!
//! # One shape for four verbs
//!
//! Create, edit, pause and delete all reduce to the same three steps: take the
//! fence, push what the row now says, release the fence with the outcome. That
//! is why there is one [`Schedules::reconcile`] rather than four flows — a
//! second flow is a second place for the fence to be released wrongly, and the
//! fence is the only thing standing between two syncers and a schedule
//! registered twice.
//!
//! # A failed push is stored, not swallowed and not raised
//!
//! The row keeps `sync_status = failed` and the reason. That is deliberate on
//! both sides: swallowing it would leave a schedule that silently never fires
//! with nothing for an operator to see, and raising it out of a create would
//! fail a request whose row was written correctly. The caller is told the
//! schedule is saved and not yet live, and `:sync` retries it.
//!
//! # What happens when the fence is lost mid-push
//!
//! Nothing, on purpose. A finalize whose generation or token no longer matches
//! updates no row and answers [`Reconciled::Superseded`]. Another syncer took
//! the schedule while this one was talking upstream, and its state is the newer
//! one — overwriting it with this attempt's outcome would resurrect a change
//! the operator had already replaced.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::Result;
use crate::model::{DesiredStatus, Schedule};
use crate::qstash::QStash;
use crate::store::Schedules as Store;

/// What one reconcile did.
///
/// Three outcomes and not a `bool`, because the caller renders each
/// differently: a synced schedule is returned to the person who edited it, a
/// failed one is returned WITH the reason so they know it is not yet live, and
/// a superseded one is not returned at all because the row they hold is stale.
#[derive(Debug, Clone)]
pub enum Reconciled {
    /// The scheduler agrees with the row.
    Synced(Schedule),
    /// The push failed and the row records why.
    Failed(Schedule),
    /// Another syncer took the schedule while this one was pushing.
    Superseded,
    /// The schedule was `deleting`, the scheduler agreed, and the row is gone.
    Removed,
}

/// The store and the scheduler, reconciled against each other.
///
/// Cheap to clone: both halves are handles over shared pools.
#[derive(Debug, Clone)]
pub struct Schedules {
    /// Where the intent lives.
    store: Store,
    /// What the intent is mirrored to.
    upstream: QStash,
}

impl Schedules {
    /// Binds the reconciler to a store and a scheduler.
    #[must_use]
    pub const fn new(store: Store, upstream: QStash) -> Self {
        Self { store, upstream }
    }

    /// The store, for the reads a route makes without reconciling.
    #[must_use]
    pub const fn store(&self) -> &Store {
        &self.store
    }

    /// Pushes a claimed schedule upstream and releases its fence.
    ///
    /// `held` must be a schedule this caller just claimed, and `token` the
    /// token it claimed with — the finalize is conditioned on both, which is
    /// what makes a lost fence a no-op rather than a clobber.
    ///
    /// # Errors
    /// Reports a datastore that would not answer or a row this build cannot
    /// read. A scheduler that refused is NOT an error here: it is recorded on
    /// the row and answered as [`Reconciled::Failed`] — see the module note.
    pub async fn reconcile(
        &self,
        held: &Schedule,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<Reconciled> {
        let pushed = match held.desired_status {
            // A paused schedule is REMOVED upstream rather than left registered
            // and ignored here. Leaving it would keep the scheduler calling a
            // fire this daemon then drops — real traffic, real cost, no effect.
            DesiredStatus::Paused | DesiredStatus::Deleting => {
                self.upstream.remove(&held.source_key).await.map(|()| None)
            }
            DesiredStatus::Active => self
                .upstream
                .upsert(&held.cron, &held.timezone, &held.message)
                .await
                .map(Some),
        };

        match pushed {
            Ok(_registered) if held.desired_status == DesiredStatus::Deleting => {
                // The row goes only now, with the scheduler already agreeing —
                // see `DesiredStatus::Deleting` on why it cannot go first.
                if self.store.delete_claimed(held, token).await? {
                    Ok(Reconciled::Removed)
                } else {
                    Ok(Reconciled::Superseded)
                }
            }
            // The scheduler's own key is ADOPTED here, not discarded. A create
            // invents a placeholder (`{fleet_id}-{millis}`) because it has not
            // spoken to the scheduler yet; this is the first moment the real key
            // exists, and the last moment it can be stored before a pause or a
            // delete has to name it. Dropping it made `remove` name a key the
            // scheduler never issued, which answers 404 — read as success by
            // design — so the row reported Removed while the schedule kept
            // firing. `None` on the pause path teaches nothing and keeps the key.
            Ok(registered) => Ok(self
                .store
                .finalize_synced(
                    held,
                    token,
                    registered.as_ref().map(|key| key.schedule_id.as_str()),
                    now,
                )
                .await?
                .map_or(Reconciled::Superseded, Reconciled::Synced)),
            Err(refused) => {
                // The sentence, not the error, and never the vendor's own body:
                // this string is read back out of the row by an operator and by
                // the dashboard, and an upstream payload could carry anything.
                let reason = refused.detail();
                let recorded = self.store.finalize_failed(held, token, reason, now).await?;
                Ok(recorded.map_or(Reconciled::Superseded, Reconciled::Failed))
            }
        }
    }
}
