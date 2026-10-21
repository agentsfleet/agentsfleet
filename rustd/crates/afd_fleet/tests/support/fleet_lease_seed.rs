//! Seeding one fleet that holds one event, with runners enrolled against it.
//!
//! The composite keys require matching fleet, workspace and tenant rows.
//! Each runner carries its fleet's placement tag to isolate parallel tests.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Leases};
use afd_runner::heartbeat::holds::prove;
use afd_wire::runner::{HeartbeatRequest, HeldFleets, NetworkPolicy, SandboxTier};

use crate::requests::{capable, enrolment_tagged, placement_tag};
use crate::support::Fixtures;

/// Distinguishes fleets created by one process, so two runs never share one.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A fleet, workspace and tenant nothing else in the lane will name.
///
/// Fresh identifiers isolate rows in the shared test database.
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
    /// The LOGICAL event id the fixture admitted under.
    ///
    /// `Acquired::event_id`, `fleet.events` and `billing.usage_lines` share it.
    pub(crate) event_id: String,
    /// The fleet holding the event.
    pub(crate) fleet: String,
    /// Its billing tenant.
    pub(crate) tenant: String,
}

/// A fleet with one admission and `N` enrolled runners.
pub(crate) async fn seeded<const N: usize>(fixtures: &Fixtures) -> Seeded<N> {
    let (fleet, workspace, tenant, runners) = seeded_parts::<N>(fixtures).await;
    let event_id = crate::queue::enqueue(
        &fixtures.database,
        &fleet,
        &workspace,
        ACTOR,
        EVENT_TYPE,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
    Seeded {
        runners,
        event_id,
        fleet,
        tenant,
    }
}

/// `runner`'s first beat, carrying the capability report a real host sends
/// before it polls.
///
/// A host that has never reported reads degraded, and a degraded host may
/// claim nothing; without this beat every seeded runner would be refused at
/// issue for a reason no lease test is about.
async fn reports_capable(fixtures: &Fixtures, runner: &Uuid7) {
    let request = HeartbeatRequest {
        capability_report: Some(capable()),
        selftest: None,
        holds: HeldFleets(Vec::new()),
        closing: false,
    };
    let held = prove(request.holds.clone());
    fixtures
        .runners()
        .heartbeat(runner, &request, held.as_ref(), fixtures.enrolled_at())
        .await
        .expect("the first beat lands");
}

/// The fleet and its runners, before any admission.
///
/// Split out of [`seeded`] because an ordering test chooses when to admit rows,
/// and [`seeded`] admits one
/// before it returns.
pub(crate) async fn seeded_parts<const N: usize>(
    fixtures: &Fixtures,
) -> (String, String, String, [Uuid7; N]) {
    let (fleet, workspace, tenant) = unique_ids();
    // One tag per seeded fleet, carried by its own runners and nobody else's:
    // see `Fixtures::seed_fleet` on why the assignment pass needs it.
    let tag = placement_tag(&fleet);
    fixtures.seed_fleet(&fleet, &workspace, &tenant, &tag).await;
    let mut runners = Vec::with_capacity(N);
    for _ in 0..N {
        let request = enrolment_tagged(
            SandboxTier::LandlockFull,
            NetworkPolicy::AllowListEgress,
            1,
            &tag,
        );
        let enrolled = fixtures
            .runners()
            .register(&request, fixtures.enrolled_at())
            .await
            .expect("enrolment must succeed");
        reports_capable(fixtures, &enrolled.runner_id).await;
        runners.push(enrolled.runner_id);
    }
    (
        fleet,
        workspace,
        tenant,
        runners
            .try_into()
            .expect("N enrolments produce exactly N identifiers"),
    )
}

/// One serving poll, prioritizing this fixture's fleet among held fleets.
pub(crate) async fn select_fleet(leases: &Leases, runner: &Uuid7, fleet: &str) -> Option<Acquired> {
    let named = [Uuid7::parse(fleet).expect("seeded fleet")];
    let acquired = leases
        .select(runner, &named)
        .await
        .expect("serving selection");
    if let Some(acquired) = &acquired {
        assert_eq!(acquired.fleet_id.as_str(), fleet);
    }
    acquired
}

/// One serving discovery pass, including its unconditional Postgres probe.
pub(crate) async fn select_once(leases: &Leases, runner: &Uuid7) -> Option<Acquired> {
    leases.select(runner, &[]).await.expect("serving selection")
}
