//! The suite's entry into the lease verb, below the one step it cannot steer.
//!
//! [`Plane::lease`] begins by asking the readiness index for work, and that
//! call peeks the partition a cursor the process shares names, plus the first
//! partition the readiness doorbell rang at or after that cursor. Neither is
//! the suite's to choose, so a suite that
//! wants its own fleet polls until that fleet comes up —
//! `integration_terminal_redelivery.rs` budgets eight rotations for it and
//! says, at length, that a loop falling through silently has been
//! misdiagnosed as the wrong bug five times.
//!
//! Everything BELOW the claim is deterministic: the same event, the same fleet,
//! the same gates, the same answer. So the suite claims the event itself —
//! `seed::select_fleet_within_rotations` already hands tests an [`Acquired`]
//! for a named fleet — and enters the verb here.
//!
//! This is a seam, not a shortcut. It runs the identical chain [`Plane::lease`]
//! runs, in the identical order, and returns the identical bytes; what it does
//! not do is decide which event to run it over. A gate proven through this
//! entry is proven for the production path, because there is only one.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::pull::Plane;

impl Plane {
    /// [`Plane::lease`] over an event the caller has already claimed.
    ///
    /// # Errors
    /// As [`Plane::lease`]: a datastore that would not answer, and a stored
    /// configuration this daemon cannot read. Every decision is an `Ok`.
    pub async fn lease_claimed(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<String> {
        self.run_claimed(acquired, runner_id, now, std::time::Instant::now())
            .await
    }
}
