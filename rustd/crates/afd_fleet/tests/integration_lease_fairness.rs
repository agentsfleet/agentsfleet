//! Discovery under a skewed population: a hot partition full of fleets no
//! runner can place does not hide the few fleets one can.
//!
//! The discovery half of the fairness dimension; the delivery half is graded
//! in `afd_outbound`'s lane suite. What is asserted here is the property the
//! partition measurement was made for: every eligible fleet is leased inside
//! ONE rotation of polls, however many ineligible fleets share the index.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::collections::BTreeSet;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::ready::{Doorbell, Partition, READY_PARTITIONS, ReadyIndex, ReadyPrefix};
use afd_fleet::lease::{Acquired, Leases};
use afd_wire::runner::{NetworkPolicy, RegisterRequest, SandboxTier};

use crate::queue;
use crate::requests;
use crate::seed;
use crate::support;

use self::requests::{ENROLLED_AT, enrolment_tagged, placement_tag};
use self::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON, unique_ids};
use self::support::Fixtures;

/// How many fleets the hot partition holds. Past the poll ceiling, so a poll
/// landing there cannot see all of them — the shape the single hash fails.
const HOT_FLEETS: u32 = 96;

/// How many fleets, each in its own cold partition, the runner can place.
const COLD_FLEETS: u16 = 3;

/// The tag every hot fleet requires and the runner does not carry.
const HOT_TAG: &str = "fixture:nobody-carries-this";

/// Polls after the first in which the rung fleet must be leased.
const LATER_POLLS: usize = 2;

/// The `nth` distinct fleet id derived from `base` that lands in `partition`.
///
/// The ids the fixtures mint are hashed like any other, so a fleet is steered
/// into a partition by varying the last hextet until the hash agrees. The
/// shape stays a version-7 identifier the store parses.
///
/// `nth` is how a caller gets SEVERAL ids in one partition, and it exists
/// because the obvious alternative silently does not work: appending another
/// hextet to `base` before calling makes a forty-character string, which
/// `base[..len - 4]` then trims back to thirty-six and re-extends to forty.
/// Postgres refuses it as `invalid input syntax for type uuid`, naming a value
/// no one wrote. The suffix belongs to this function alone.
fn fleet_in(base: &str, partition: Partition, nth: usize) -> String {
    let stem = &base[..base.len() - 4];
    (0..u16::MAX)
        .map(|n| format!("{stem}{n:04x}"))
        .filter(|candidate| Partition::of(candidate) == partition)
        .nth(nth)
        .expect("every partition holds many suffixes")
}

/// The identifiers one run's fleets are minted from.
struct Population {
    base: String,
    workspace: String,
    tenant: String,
}

impl From<(String, String, String)> for Population {
    fn from((base, workspace, tenant): (String, String, String)) -> Self {
        Self {
            base,
            workspace,
            tenant,
        }
    }
}

impl Population {
    /// The hot partition: many ready fleets the runner cannot place.
    async fn seed_hot(&self, fixtures: &Fixtures) {
        let hot = Partition::new(0).expect("the first partition exists");
        for n in 0..HOT_FLEETS {
            let fleet = fleet_in(
                &self.base,
                hot,
                usize::try_from(n).expect("the hot count fits a usize"),
            );
            fixtures
                .seed_fleet(&fleet, &self.workspace, &self.tenant, HOT_TAG, ENROLLED_AT)
                .await;
            queue::mark_ready(fixtures.queue(), &fleet).await;
        }
    }

    /// The cold partitions: one placeable fleet each, holding an event.
    /// Answers the fleets and an enrolment carrying every one of their tags.
    async fn seed_cold(&self, fixtures: &Fixtures) -> (BTreeSet<String>, RegisterRequest<'static>) {
        let mut request = enrolment_tagged(
            SandboxTier::LandlockFull,
            NetworkPolicy::AllowListEgress,
            1,
            &placement_tag(&self.base),
        );
        let mut expected = BTreeSet::new();
        for index in 1..=COLD_FLEETS {
            let partition = Partition::new(index).expect("a low partition exists");
            let fleet = fleet_in(&self.base, partition, 0);
            let tag = placement_tag(&fleet);
            fixtures
                .seed_fleet(&fleet, &self.workspace, &self.tenant, &tag, ENROLLED_AT)
                .await;
            request.labels.push(Cow::Owned(tag));
            queue::enqueue(
                fixtures.queue(),
                &fleet,
                &self.workspace,
                ACTOR,
                EVENT_TYPE,
                REQUEST_JSON,
                ENROLLED_AT,
            )
            .await;
            expected.insert(fleet);
        }
        (expected, request)
    }
}

/// Skewed eligibility cannot starve unrelated work beyond one rotation.
///
/// One partition is filled with fleets whose required tag no runner carries,
/// all marked ready; three placeable fleets sit in three other partitions
/// with an event each. A runner polling one rotation leases all three.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_skewed_workload_preserves_discovery_and_delivery_fairness() {
    let fixtures = Fixtures::create_with_queue().await;
    let population = Population::from(unique_ids());
    population.seed_hot(&fixtures).await;
    let (expected, request) = population.seed_cold(&fixtures).await;
    let runner = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed")
        .runner_id;

    let store = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let mut leased = BTreeSet::new();
    for _poll in 0..READY_PARTITIONS {
        if let Some(acquired) = store
            .select(&runner, &[], now)
            .await
            .expect("a poll must not fault")
        {
            leased.insert(acquired.fleet_id.as_str().to_owned());
        }
    }

    for fleet in &expected {
        queue::clear_ready(fixtures.queue(), fleet).await;
    }
    fixtures.cleanup().await;
    assert_eq!(
        leased, expected,
        "one rotation of polls must lease every placeable fleet, hot partition or not"
    );
}

/// A poll the rotation sends to a partition with work leases that work before
/// the doorbell's: the rung partition is rung again, because its fleet was
/// never tried, and a later poll leases it.
///
/// A private index, so this run's partitions hold only its own two marks,
/// and a fresh store, so the cursor's first partition is 0. The store's
/// doorbell holds partition 5 rung, so the first poll peeks it while the
/// cursor names the other fleet's partition. The rung fleet is also sticky to
/// the runner, so only the cursor-first ordering puts the other ahead.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_cursor_partition_leases_before_the_rung_one() {
    let fixtures = Fixtures::create_with_queue().await;
    let population = Population::from(unique_ids());
    let run = afd_core::clock::now().as_millis();
    let prefix = ReadyPrefix::private(&format!("cursor-first-{}-{run}", std::process::id()));
    let index = ReadyIndex::under(fixtures.queue().clone(), prefix.clone());
    let rung = Partition::new(5).expect("a partition");
    let behind_the_ring = fleet_in(&population.base, rung, 0);
    let cursor = fleet_in(&population.base, Partition::new(0).expect("a partition"), 0);
    let mut request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        1,
        &placement_tag(&population.base),
    );
    for fleet in [&behind_the_ring, &cursor] {
        seed_marked(&fixtures, &index, &population, fleet).await;
        request.labels.push(Cow::Owned(placement_tag(fleet)));
    }
    let runner = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed")
        .runner_id;
    fixtures
        .make_sticky(&behind_the_ring, runner.as_str(), ENROLLED_AT)
        .await;
    let doorbell = Doorbell::new();
    doorbell.ring(rung);
    let store = fixtures
        .leases()
        .with_ready_prefix(prefix)
        .with_doorbell(doorbell.clone());
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let first = store
        .select(&runner, &[], now)
        .await
        .expect("a poll must not fault");
    let rung_again = doorbell.is_rung(rung);
    let later = leased_within(&store, &runner, now, LATER_POLLS).await;

    for fleet in [&behind_the_ring, &cursor] {
        queue::clear_ready(fixtures.queue(), fleet).await;
    }
    fixtures.cleanup().await;
    assert_eq!(
        first.map(|acquired| acquired.fleet_id.as_str().to_owned()),
        Some(cursor),
        "the cursor's partition is claimed before the rung one's"
    );
    assert!(rung_again, "the untried partition is rung again");
    assert_eq!(
        later.map(|acquired| acquired.fleet_id.as_str().to_owned()),
        Some(behind_the_ring),
        "a later poll takes the ring and leases the fleet behind it"
    );
}

/// Seeds `fleet` into `population`, queues one event for it and marks it.
async fn seed_marked(
    fixtures: &Fixtures,
    index: &ReadyIndex,
    population: &Population,
    fleet: &str,
) {
    let tag = placement_tag(fleet);
    fixtures
        .seed_fleet(
            fleet,
            &population.workspace,
            &population.tenant,
            &tag,
            ENROLLED_AT,
        )
        .await;
    queue::enqueue(
        fixtures.queue(),
        fleet,
        &population.workspace,
        ACTOR,
        EVENT_TYPE,
        REQUEST_JSON,
        ENROLLED_AT,
    )
    .await;
    index
        .mark(fleet)
        .await
        .expect("the private index takes a mark");
}

/// The first lease `store` hands `runner` within `polls` polls.
async fn leased_within(
    store: &Leases,
    runner: &Uuid7,
    now: UnixMillis,
    polls: usize,
) -> Option<Acquired> {
    for _poll in 0..polls {
        let leased = store
            .select(runner, &[], now)
            .await
            .expect("a poll must not fault");
        if leased.is_some() {
            return leased;
        }
    }
    None
}
