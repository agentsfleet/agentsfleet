//! Test entry for running the production gate and issue sequence over a selected admission.

use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::pull::LeaseService;

impl LeaseService {
    /// Run the production gate and issue sequence over an admission already selected by the test.
    ///
    /// # Errors
    /// Preserves datastore, configuration and payload encoding failures.
    pub async fn claim_selected(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: Timestamp,
    ) -> Result<Option<serde_json::Value>> {
        let mut encode = |lease: af_wire::lease::LeasePayload<'_>,
                          _metadata: crate::lease::Offered| {
            serde_json::to_value(af_wire::lease::TaggedLease::<serde_json::Value>::Fleet(
                Box::new(lease),
            ))
            .map_err(af_core::json::Unreadable::from)
        };
        Ok(self
            .run_claimed_with(
                acquired,
                runner_id,
                now,
                std::time::Instant::now(),
                &mut af_observability::verb_span::VerbClock::untimed(),
                &mut encode,
            )
            .await?
            .transpose()?)
    }
}
