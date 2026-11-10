//! Closed triggers of an outbound claim; no destination or workspace becomes a series.
use super::closed_set;

closed_set! {
    /// What made a worker claim.
    ClaimVia {
        /// A committed report's ping.
        Ping => "ping",
        /// The periodic pass, or a full batch draining a backlog.
        Scan => "scan"
    }
}
