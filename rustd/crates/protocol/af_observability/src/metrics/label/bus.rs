//! Closed causes of a bus gap; no subject, fleet or draft becomes a series.
use super::closed_set;

closed_set! {
    /// Why a subscription was told frames may be missing.
    GapCause {
        /// The client lost its server and found another.
        Reconnect => "reconnect",
        /// The client's buffer for a subscription overflowed.
        SlowConsumer => "slow_consumer",
        /// A publisher owed a marker after a failed or interrupted publish.
        PublishDebt => "publish_debt"
    }
}
