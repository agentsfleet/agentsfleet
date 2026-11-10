//! The authoritative facts returned by a settled report.

use super::RunTokens;
use af_core::id::Uuid7;
use afd_billing::Nanos;
use jiff::Timestamp;

/// What one settled report leaves its caller with.
///
/// A struct rather than a bare [`Nanos`] because the charge was never the only
/// thing the call learned. The report names a LEASE; everything else about the
/// run — which fleet and workspace and tenant, which event, which model under
/// which posture — was resolved by the load inside, and the caller would
/// otherwise run a second statement for facts this call already held.
///
/// # Why the identity travels up rather than the telemetry down
///
/// Both callers of this verb describe the finished run: one reports it to
/// product analytics, the other records its delivery span. Neither concern
/// belongs on the money path — an exporter fused into it would be one a report
/// could not settle without — so the money path answers with the facts and the
/// handler decides what to say about them.
///
/// Exhaustive on purpose, where most of this crate's public types are not: a
/// suite stubbing the lease plane has to ANSWER with one of these, and a
/// `non_exhaustive` struct cannot be built outside the crate that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciled {
    /// What the final slice drained. Zero on a repeated report, which charges
    /// nothing because the first one charged it.
    pub charged: Nanos,
    /// The fleet that ran.
    pub fleet_id: Uuid7,
    /// The workspace it belongs to.
    pub workspace_id: Uuid7,
    /// The tenant whose wallet the settle drew on.
    pub tenant_id: Uuid7,
    /// The event that was executed, as the LEASE row records it.
    ///
    /// The lease's copy, never the request's. A runner names an event in its
    /// report and this is the one the lease was issued for; where they could
    /// differ, the row is the authority.
    pub event_id: Uuid7,
    /// The billing posture the lease was issued under, in the stored spelling.
    pub posture: String,
    /// The provider resolved at issue, as configured.
    pub provider: String,
    /// The model resolved at issue.
    pub model: String,
    /// Whether this call produced the outcome or found it already durable.
    ///
    /// True when the runner re-sent a report whose response it never received.
    /// The run itself happened once, so everything a caller says ABOUT the run
    /// — the completion analytics, the cost meters, the delivery span — must
    /// fire on the first report and not on the repeat, or one run is counted
    /// as many. The report still answers success: a runner told to retry
    /// forever is how a finished answer gets thrown away.
    pub repeated: bool,
    /// The tokens the run's usage lines record, every renewal's and the
    /// report's: what its cost metrics and delivery span describe.
    pub tokens: RunTokens,
    /// When the event was created, as the lease row carries it.
    pub event_created_at: Timestamp,
    /// When the lease was issued.
    pub lease_created_at: Timestamp,
    /// When `event_complete` was published; none on a repeated report, and
    /// none when the settle closed no event row.
    pub completed_at: Option<Timestamp>,
}
