//! What the assignment pass SAYS when something goes wrong.
//!
//! Split from `assign.rs` when the run-start counter pushed that file past the
//! length cap, and split HERE rather than anywhere else because these are one
//! concern: the pass choosing work is a different question from the pass
//! reporting why it could not. Every item is a `warn` and none of them changes
//! what the poll returns — a runner recovers by backing off and re-polling, so
//! nothing here is raised to it.

use afd_core::error_code;
use afd_core::id::Uuid7;

/// The readiness index would not answer.
///
/// These five are `LOGGING_STANDARD.md` §3 `event` values — `snake_case`
/// `verb_noun`, one declaration each (RULE UFS), and byte-identical to the
/// spellings `assign.zig` emits so a dashboard built against the Zig daemon
/// keeps matching after the cutover.
pub(super) const EVENT_READY_PEEK_FAILED: &str = "assign_ready_peek_failed";

/// An entry no reader can decode was acknowledged and discarded.
const EVENT_ENTRY_UNDECODABLE_DROPPED: &str = "assign_entry_undecodable_dropped";

/// The discard could not be acknowledged; the next poll tries again.
const EVENT_ENTRY_UNDECODABLE_DROP_FAILED: &str = "assign_entry_undecodable_drop_failed";

/// A lapsed holder's event was taken back under a higher fence.
pub(super) const EVENT_LEASE_RECLAIMED: &str = "lease_reclaimed";

/// Reports a queue failure that ended a poll before any fleet was examined.
///
/// A `warn` rather than an `err` because the runner recovers on its own: it
/// backs off and re-polls, and the work stays leasable. It is emitted rather
/// than left to the caller because `LOGGING_STANDARD.md` §4 is explicit that a
/// path which can fail logs its failure — and this one propagates, so without
/// this line the only record would be whatever the handler chose to say.
pub(super) fn warn_queue(event: &'static str, runner_id: &Uuid7, error: &afd_dragonfly::Error) {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let runner = runner_id.as_str();
    let reason = error.to_string();
    tracing::warn!(
        error_code = code,
        event,
        runner_id = runner,
        reason,
        "the lease poll ended early; the runner backs off and re-polls"
    );
}

/// Reports a queue failure against one fleet's stream.
pub(crate) fn warn_queue_fleet(event: &'static str, fleet_id: &str, error: &afd_dragonfly::Error) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = error.to_string();
    tracing::warn!(
        error_code = code,
        event,
        fleet_id,
        reason,
        "the fleet's stream could not be read; its claim is not converted to a lease"
    );
}

/// Acknowledges an entry no reader can decode, so the fleet keeps moving.
///
/// Without this the entry is PERMANENT, and it takes the fleet with it.
/// `XREADGROUP >` never re-offers a delivered entry; the reclaim sweeper
/// (`afd_runner::sweep::reclaim`) claims it back into this consumer on every
/// pass; and pending-first hands it straight back as the oldest entry on every
/// poll that wins the fleet. So one malformed write makes that fleet
/// permanently unleasable and hides every event queued behind it — which is
/// exactly the shape of the cutover defect this branch fixes, and would have
/// outlived the fix for any stream still holding one.
///
/// `afd_dragonfly::outbound`'s `drop_undeliverable` is the same answer for the
/// other stream, written for the same reason.
///
/// A `warn` rather than an `err`: the daemon recovers by itself, so nothing is
/// raised to the runner, and this line is the only record the entry existed —
/// which is why it names the fleet, the entry and the field. A failed
/// acknowledgement is not raised either; the entry stays pending and the next
/// poll drops it again.
pub(super) async fn drop_undecodable(
    streams: &afd_dragonfly::FleetStreams,
    fleet_id: &str,
    receipt: &afd_dragonfly::EventId,
    error: &crate::error::Error,
) {
    let id = receipt.as_str();
    let reason = error.to_string();
    let event = if streams.ack(fleet_id, receipt).await.is_ok() {
        EVENT_ENTRY_UNDECODABLE_DROPPED
    } else {
        EVENT_ENTRY_UNDECODABLE_DROP_FAILED
    };
    tracing::warn!(
        error_code = error_code::INTERNAL_OPERATION_FAILED.as_str(),
        event,
        fleet_id,
        receipt = id,
        reason,
        "a stream entry no reader can decode was discarded so the fleet stays leasable"
    );
}
