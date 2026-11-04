//! Closed scheduler decisions; no schedule identifiers become series.
use super::closed_set;
closed_set! {
    /// Terminal answer to one verified callback.
    FireOutcome {
        /// New work was admitted.
        Accepted => "accepted",
        /// An earlier callback already admitted the work.
        Replayed => "replayed",
        /// A paused schedule or fleet was skipped.
        Skipped => "skipped",
        /// The callback names no current schedule.
        Stale => "stale",
        /// A dependency refused processing.
        Failed => "failed",
        /// The caller stopped before an answer.
        Cancelled => "cancelled"
    }
}
closed_set! {
    /// Completed reconciliation decisions.
    SyncOutcome {
        /// Upstream and stored intent agree.
        Synced => "synced",
        /// The upstream rejected its update.
        Failed => "failed",
        /// A newer fence owns the row.
        Superseded => "superseded",
        /// A deletion or pause removed the upstream schedule.
        Removed => "removed",
        /// A one-shot missed its registered instant.
        Expired => "expired",
        /// The datastore refused the reconciliation.
        Error => "error"
    }
}
closed_set! {
    /// Seven independently counted schedule states.
    ScheduleState {
        /// Active intent.
        Active => "active",
        /// Paused intent.
        Paused => "paused",
        /// Pending removal.
        Deleting => "deleting",
        /// Upstream agrees.
        Synced => "synced",
        /// Reconciliation holds a live or expired fence.
        Syncing => "syncing",
        /// Last upstream update failed.
        Failed => "failed",
        /// An active one-shot is overdue.
        OverdueOnce => "overdue_once"
    }
}
