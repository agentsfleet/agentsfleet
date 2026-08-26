//! Seeding one fleet that holds one event, with runners enrolled against it.
//!
//! Shared by the lease suites because they all need the same precondition and
//! it is fiddly: a fleet needs its workspace and tenant to exist and to AGREE
//! (the schema ties them with a composite key), the stream needs a consumer
//! group before a read, and the readiness index needs a mark or the poll never
//! looks. Any one of those missing makes the assignment pass look broken when
//! it is the fixture that is.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use crate::requests::{ENROLLED_AT, enrolment};
use crate::support::Fixtures;

/// Distinguishes fleets created by one process, so two runs never share one.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A fleet, workspace and tenant nothing else in the lane will name.
///
/// Fresh per test rather than constant, and that is not tidiness. The database
/// is per-test, but REDIS IS NOT: the readiness index is one hash at a fixed
/// key and a fleet's stream is keyed by its id, so a constant fleet id makes
/// every run inherit the previous run's stream entries. The first version of
/// this suite did exactly that and failed asserting a stale entry id — the
/// stream already held earlier runs' events and the pass correctly returned
/// the oldest.
///
/// Shaped so the schema's `uuidv7` CHECK passes: the character after the
/// second dash must be `7`.
pub(crate) fn unique_ids() -> (String, String, String) {
    let run = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let id = |slot: u32| format!("0195b4ba-8d3a-7{run:03x}-8abc-{pid:08x}{slot:04x}");
    (id(1), id(2), id(3))
}

/// The actor every seeded event carries.
pub(crate) const ACTOR: &str = "fixture:steer";

/// The event type every seeded event carries.
pub(crate) const EVENT_TYPE: &str = "steer";

/// The body every seeded event carries.
pub(crate) const REQUEST_JSON: &str = "{\"prompt\":\"fixture\"}";

/// The billing posture an issued fixture lease records.
pub(crate) const POSTURE: &str = "platform";

/// The provider an issued fixture lease records.
pub(crate) const PROVIDER: &str = "anthropic";

/// The model an issued fixture lease records.
pub(crate) const MODEL: &str = "claude-fixture";

/// What one seeded fleet hands back.
///
/// A struct rather than a tuple: four values of which three are `String` is
/// exactly the shape where a caller silently binds them in the wrong order.
pub(crate) struct Seeded<const N: usize> {
    /// The enrolled runners, ready to destructure.
    pub(crate) runners: [Uuid7; N],
    /// The entry id the append produced.
    pub(crate) event_id: String,
    /// The fleet holding the event.
    pub(crate) fleet: String,
    /// Its billing tenant.
    pub(crate) tenant: String,
}

/// A fleet with one event on its stream, and `N` enrolled runners.
pub(crate) async fn seeded<const N: usize>(fixtures: &Fixtures) -> Seeded<N> {
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, ENROLLED_AT)
        .await;
    let mut runners = Vec::with_capacity(N);
    for _ in 0..N {
        let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress, 1);
        let enrolled = fixtures
            .runners()
            .register(&request, UnixMillis::from_millis(ENROLLED_AT))
            .await
            .expect("enrolment must succeed");
        runners.push(enrolled.runner_id);
    }
    let event_id = crate::queue::enqueue(
        fixtures.queue(),
        &fleet,
        &workspace,
        ACTOR,
        EVENT_TYPE,
        REQUEST_JSON,
        ENROLLED_AT,
    )
    .await;
    Seeded {
        runners: runners
            .try_into()
            .expect("N enrolments produce exactly N identifiers"),
        event_id,
        fleet,
        tenant,
    }
}
