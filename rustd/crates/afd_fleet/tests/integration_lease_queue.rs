//! The lease queue: readiness kept on the slot by the admission trigger, one claim statement, fenced one-statement endings and the
//! poll key's replay.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use jiff::Timestamp;

use crate::rows::Row;
use crate::support::Fixtures;

#[path = "integration_lease_queue/approvals.rs"]
mod approvals;
#[path = "integration_lease_queue/contention.rs"]
mod contention;
#[path = "integration_lease_queue/endings.rs"]
mod endings;
#[path = "integration_lease_queue/readiness.rs"]
mod readiness;
#[path = "integration_lease_queue/replay.rs"]
mod replay;

/// One admission on `fleet`, stamped `offset` past the fixture's instant.
async fn admit_at(fixtures: &Fixtures, fleet: &str, workspace: &str, offset: u64) -> String {
    crate::queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        crate::seed::ACTOR,
        crate::seed::EVENT_TYPE,
        crate::seed::REQUEST_JSON,
        afd_core::clock::after(fixtures.enrolled_at(), Duration::from_secs(offset)),
    )
    .await
}

/// The fleet's slot readiness, or `None` for a NULL or no slot.
async fn ready_at(fixtures: &Fixtures, fleet: &str) -> Option<Timestamp> {
    fixtures.instant(Row::Slot(fleet), "ready_at").await
}

/// An admission's own `created_at`.
async fn created_at(fixtures: &Fixtures, admission: &str) -> Option<Timestamp> {
    fixtures
        .instant(Row::Admission(admission), "created_at")
        .await
}

/// The fleet's slot fence, or `None` with no slot.
async fn fence(fixtures: &Fixtures, fleet: &str) -> Option<i64> {
    fixtures
        .value::<i64>(Row::Slot(fleet), "fencing_token")
        .await
}
