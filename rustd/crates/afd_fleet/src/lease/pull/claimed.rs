//! Test entry for running the production gate and issue sequence over an already claimed admission.

use afd_core::id::Uuid7;
use jiff::Timestamp;

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
        now: Timestamp,
    ) -> Result<String> {
        self.run_claimed(acquired, runner_id, now, std::time::Instant::now())
            .await
            .map(super::Polled::into_body)
    }
}
