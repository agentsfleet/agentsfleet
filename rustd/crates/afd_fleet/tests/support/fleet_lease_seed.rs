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
use afd_datastore::ready::READY_PARTITIONS;
use afd_fleet::lease::{Acquired, Leases};
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use crate::requests::{ENROLLED_AT, enrolment_tagged, placement_tag};
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
    /// The LOGICAL event id the fixture admitted under.
    ///
    /// Not the entry id: the two are different things now that the admission
    /// ledger owns identity, and it is this
    /// one that `Acquired::event_id`, `core.fleet_events` and the usage ledger
    /// all address. A test comparing against the receipt would be comparing
    /// against the wrong half.
    pub(crate) event_id: String,
    /// The fleet holding the event.
    pub(crate) fleet: String,
    /// Its billing tenant.
    pub(crate) tenant: String,
}

/// A fleet with one event on its stream, and `N` enrolled runners.
pub(crate) async fn seeded<const N: usize>(fixtures: &Fixtures) -> Seeded<N> {
    let (fleet, workspace, tenant, runners) = seeded_parts::<N>(fixtures).await;
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
        runners,
        event_id,
        fleet,
        tenant,
    }
}

/// The fleet and its runners, with NOTHING on the stream yet.
///
/// Split out of [`seeded`] because a test about which entry a poll meets first
/// has to choose the order entries are appended in, and [`seeded`] appends one
/// before it returns.
pub(crate) async fn seeded_parts<const N: usize>(
    fixtures: &Fixtures,
) -> (String, String, String, [Uuid7; N]) {
    let (fleet, workspace, tenant) = unique_ids();
    // One tag per seeded fleet, carried by its own runners and nobody else's:
    // see `Fixtures::seed_fleet` on why the assignment pass needs it.
    let tag = placement_tag(&fleet);
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, &tag, ENROLLED_AT)
        .await;
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
            .register(&request, UnixMillis::from_millis(ENROLLED_AT))
            .await
            .expect("enrolment must succeed");
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

/// One rotation of assignment polls, answering the first work any of them took.
///
/// **A single `select` is not a poll of the deployment, it is a poll of one
/// PARTITION.** Readiness is spread over [`READY_PARTITIONS`] hashes
/// and each pass rotates the cursor by one, so a fixture holding one fleet is
/// visited by one poll in sixteen. Every suite here that called `select` once
/// and unwrapped it was passing on a one-in-sixteen draw, which is how thirty
/// four integration tests failed the first time this lane ran after readiness
/// was partitioned.
///
/// One rotation is the bound the partition count was measured against: every marked fleet is discoverable within it. A loop without that
/// bound would hang on a fixture whose fleet is genuinely not leasable, which
/// is a thing several suites deliberately assert.
///
/// `None` therefore means what the old single call was trying to mean: no
/// partition holds leasable work for this runner.
/// [`select_within_one_rotation`], narrowed to ONE fleet's work.
///
/// The readiness index is global and the lane resets once per RUN rather than
/// per test, so a rotation started here can acquire a fleet an earlier test
/// left marked. Handing that back to a caller asserting on its own admission
/// is how `test_cluster_restart_and_stale_snapshot_preserve_obligations` came
/// to compare two unrelated event ids -- green alone, red in a full run, with
/// no change in between.
///
/// Polls until this fleet's slot is the one acquired. A slot belonging to
/// someone else is passed over, which does lease it; those are fleets from
/// tests that already finished, and the next run's reset clears them. The
/// budget is several rotations rather than one because residue can hold many
/// partitions at once, and `None` still means the fleet was genuinely never
/// offered rather than that the walk was too short.
pub(crate) async fn select_fleet_within_rotations(
    leases: &Leases,
    runner: &Uuid7,
    now: UnixMillis,
    fleet: &str,
) -> Option<Acquired> {
    const ROTATIONS: u16 = 8;
    for _poll in 0..(READY_PARTITIONS * ROTATIONS) {
        if let Some(acquired) = leases
            .select(runner, now)
            .await
            .expect("the assignment pass must not fault")
            && acquired.fleet_id.to_string() == fleet
        {
            return Some(acquired);
        }
    }
    None
}

pub(crate) async fn select_within_one_rotation(
    leases: &Leases,
    runner: &Uuid7,
    now: UnixMillis,
) -> Option<Acquired> {
    for _poll in 0..READY_PARTITIONS {
        if let Some(acquired) = leases
            .select(runner, now)
            .await
            .expect("the assignment pass must not fault")
        {
            return Some(acquired);
        }
    }
    None
}
