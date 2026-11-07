//! Fixed pool roles and terminal acquisition outcomes.

use super::closed_set;

closed_set! {
    /// Independently authenticated connection pools.
    PoolRole {
        /// Background work.
        Default => "default",
        /// API requests.
        Api => "api",
        /// Schema migration sessions.
        Migrator => "migrator"
    }
}
closed_set! {
    /// One logical acquisition, including its permitted retry.
    AcquireOutcome {
        /// A connection was returned.
        Acquired => "acquired",
        /// Every established connection remained busy.
        Capacity => "capacity",
        /// Connection establishment exhausted its retry.
        Stalled => "stalled",
        /// The datastore refused the acquisition.
        Unavailable => "unavailable",
        /// The caller dropped its pending request.
        Cancelled => "cancelled"
    }
}
closed_set! {
    /// Connections and the configured ceiling are distinct readings.
    PoolState {
        /// Established connections.
        Open => "open",
        /// Available established connections.
        Idle => "idle",
        /// Checked-out connections.
        Used => "used",
        /// Configured ceiling.
        Capacity => "capacity",
        /// Pending logical acquisitions.
        Waiting => "waiting"
    }
}
