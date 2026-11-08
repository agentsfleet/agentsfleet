//! A heartbeat held until fresh work is admitted.
//!
//! ```text
//!   beat ─► liveness, capability, holds recorded ─► wake asked?
//!     no  ─► answer woken: false
//!     yes ─► fewer than HELD_BEATS_MAX held? ─► wait for a ring, up to WAKE_HOLD_MAX
//!              ring     ─► woken: true   (one idle worker polls now)
//!              deadline ─► woken: false  (the hold was the interval)
//!            no ─► woken: false at once, counted and logged
//! ```
//!
//! The hold comes after the beat's own writes, so a held beat has already
//! counted as alive, and it ends no later than the runner's next beat was due.
//! A ring is the doorbell's: a Dragonfly sharded publish every replica's hub
//! hears, so a beat held here wakes for an admission committed on any replica.
//! A ring that is lost costs only the hold; the runner's idle hint still polls.

use af_core::limits::HELD_BEATS_MAX;
use af_observability::metrics::label::fleet::WakeAnswer;
use af_observability::producers;
use afd_fleet::lease::Waited;
use afd_http::admission::DEFAULT_MAX_IN_FLIGHT;

/// A beat past the cap, answered at once.
pub(super) const EVENT_WAKE_REFUSED: &str = "runner_wake_refused";

// A held beat keeps an in-flight permit for its whole hold; the cap must leave
// most of them to requests.
const _: () = assert!(
    HELD_BEATS_MAX * 4 <= DEFAULT_MAX_IN_FLIGHT.get(),
    "held beats may take at most a quarter of the API's in-flight permits"
);

/// The `woken` a beat answers with: held through `wait` when it asked, which
/// ends on a ring, the deadline or the cap; `false` without waiting when it
/// did not. Every reply carries it, so its presence is the runner's proof this
/// daemon holds beats.
pub(super) async fn woken<F>(asked: bool, runner_id: &str, wait: impl FnOnce() -> F) -> bool
where
    F: Future<Output = Waited>,
{
    if !asked {
        return false;
    }
    let waited = wait().await;
    let answer = match waited {
        Waited::Rung => WakeAnswer::Ring,
        Waited::Deadline => WakeAnswer::Deadline,
        Waited::Capped => WakeAnswer::Capped,
    };
    producers::fleet::wake_answered(answer);
    if waited == Waited::Capped {
        let cap = HELD_BEATS_MAX;
        let event = EVENT_WAKE_REFUSED;
        tracing::debug!(
            runner_id,
            cap,
            event,
            "the beat was not held: this process holds its cap of beats"
        );
    }
    waited == Waited::Rung
}

#[cfg(test)]
#[path = "wake_tests.rs"]
mod tests;
