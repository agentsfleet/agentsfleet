//! Delivering what the exporters still hold, for a caller with no supervisor.
//!
//! The supervised shutdown flush has a task and a cancellation token to hang
//! itself on. A boot that fails has neither: it is about to render a fault and
//! exit, and whatever the exporters buffered on the way down goes with it
//! unless something delivers it first.

use std::time::Duration;

use super::Exports;

/// Delivers every buffered signal, or says which part did not make it out.
///
/// [`Exports::flush`] parks the thread it runs on. It walks four providers in
/// sequence, the span and log processors wait on their own deadlines, and both
/// metric readers wait on a channel whose only bound is the operator's OTLP
/// timeout. So it goes to the blocking pool, where a parked thread is not a
/// reactor worker, and under `budget`, because a process on its way out must
/// still get out.
///
/// The budget belongs to the caller rather than to this function: shutdown's
/// has to stay under the supervisor's join deadline, and a boot failure has no
/// join to protect — only an operator waiting on a container that will not
/// start.
///
/// Reports rather than raises, for [`Exports::flush`]'s own reason: the caller
/// is already leaving, and a lost batch is worth a line, not a second fault.
pub async fn flush_within(exports: Exports, budget: Duration) {
    match tokio::time::timeout(budget, tokio::task::spawn_blocking(move || exports.flush())).await {
        Ok(Ok(())) => {}
        Ok(Err(_panicked)) => tracing::warn!(
            event = "telemetry_flush_abandoned",
            "a telemetry flush did not run to completion"
        ),
        Err(_elapsed) => tracing::warn!(
            budget_ms = budget.as_millis(),
            event = "telemetry_flush_timed_out",
            "a telemetry flush outran its budget — some telemetry was not delivered"
        ),
    }
}
