//! Continue-or-stop value for the lease admission sequence.

/// The single stable reason logged by both waiting-for-approval paths.
pub(super) const AWAITING_APPROVAL: &str = "a human owes an answer";

/// Either the pass continues, or it already has its answer.
///
/// Every ending has the same serialized shape. Carrying that shape as a value
/// keeps each decision local and prevents a caller from forgetting which stop
/// already wrote a terminal row.
pub(super) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop; these are the bytes.
    Stop(String),
}
