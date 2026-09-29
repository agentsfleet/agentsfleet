//! The readiness index as a lease poll leaves it: a drained fleet loses its
//! mark, a mark written while a poll runs survives it, an entry pending under
//! another consumer is delivered rather than stranded, and a slot a live
//! runner holds costs no claim.
//!
//! Each test polls a PRIVATE index holding its one fleet, so a rotation of
//! polls visits exactly the partition that matters and no other suite's mark
//! is in reach.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::LEASE_TTL_MS;
use afd_dragonfly::ready::READY_PARTITIONS;
use afd_dragonfly::{FleetStreams, ReadyIndex, ReadyPrefix};
use afd_fleet::lease::{Acquired, Leases};
use afd_observability::test_util::Capture;

use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{ACTOR, EVENT_TYPE, REQUEST_JSON, seeded_parts, select_within_one_rotation};
use crate::support::Fixtures;

/// The counter a won claim that found nothing moves.
const CLAIMS_EMPTY: &str = "agentsfleet_lease_claims_empty_total";

/// How many times the ingress-during-poll race is run.
const RACES: usize = 24;

/// A consumer no process reads under any more: a replica that died holding an
/// entry it had read and not yet leased.
const DEAD_REPLICA: &str = "agentsfleetd-dead-replica";

/// One fleet, two runners, and the private index a store polls.
struct Scene {
    fixtures: Fixtures,
    fleet: String,
    fleet_id: Uuid7,
    workspace: String,
    runners: [Uuid7; 2],
    leases: Leases,
    index: ReadyIndex,
}

/// A fleet with a consumer group and nothing on its stream.
async fn scene() -> Scene {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _tenant, runners) = seeded_parts::<2>(&fixtures).await;
    FleetStreams::new(fixtures.queue().clone())
        .ensure_group(&fleet)
        .await
        .expect("the consumer group must exist before a read");
    let prefix = ReadyPrefix::private(&fleet);
    Scene {
        leases: fixtures.leases().with_ready_prefix(prefix.clone()),
        index: ReadyIndex::under(fixtures.queue().clone(), prefix),
        fleet_id: Uuid7::parse(&fleet).expect("the fixture id is a v7 spelling"),
        fixtures,
        fleet,
        workspace,
        runners,
    }
}

impl Scene {
    /// Ingress: one event appended, then the fleet marked in the private index.
    async fn send(&self) -> String {
        let event_id = queue::enqueue(
            self.fixtures.queue(),
            &self.fleet,
            &self.workspace,
            ACTOR,
            EVENT_TYPE,
            REQUEST_JSON,
            ENROLLED_AT,
        )
        .await;
        self.index
            .mark(&self.fleet)
            .await
            .expect("the private index takes a mark");
        event_id
    }

    /// One rotation of polls by the first runner at `now`.
    async fn poll(&self, now: i64) -> Option<Acquired> {
        select_within_one_rotation(&self.leases, &self.runners[0], UnixMillis::from_millis(now))
            .await
    }

    /// Whether the fleet holds a mark in the private index.
    async fn marked(&self) -> bool {
        self.index
            .token_for(&self.fleet)
            .await
            .expect("the private index is readable")
            .is_some()
    }

    /// The run behind `acquired` finishes: its entry acknowledged, its claim
    /// freed at `now`.
    async fn finish(&self, acquired: &Acquired, now: i64) {
        self.leases
            .acknowledge(&self.fleet_id, &acquired.receipt)
            .await
            .expect("the entry acknowledges");
        self.leases
            .release(&self.fleet_id, acquired.fence, UnixMillis::from_millis(now))
            .await
            .expect("the claim releases");
    }

    async fn cleanup(self) {
        let _cleared = self.index.force_clear(&self.fleet).await;
        queue::clear_ready(self.fixtures.queue(), &self.fleet).await;
        self.fixtures.cleanup().await;
    }
}

/// A mark ingress writes while a poll runs survives that poll's clear, and
/// the event behind it is leased on the next poll.
///
/// Ingress takes no claim, so its append and mark can land anywhere inside a
/// poll that is about to find the fleet empty. The clear compares the
/// generation the poll peeked, so a newer mark is never the one it removes.
/// Raced here rather than staged, because the window is inside one call; the
/// assertion is the invariant, whichever order a race took.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_mark_written_during_poll_survives() {
    let capture = Capture::install();
    let scene = scene().await;
    let mut now = ENROLLED_AT;

    // A drained fleet: marked, and nothing owed. The poll that finds it so
    // clears the mark, and counts the claim that found nothing.
    scene.index.mark(&scene.fleet).await.expect("mark");
    let before = capture.sum(CLAIMS_EMPTY, &[]);
    assert!(
        scene.poll(now).await.is_none(),
        "a drained fleet leases nothing"
    );
    assert!(!scene.marked().await, "a drained fleet's mark is cleared");
    let after = capture.sum(CLAIMS_EMPTY, &[]);
    assert!(
        after > before,
        "the empty claim is counted: {before} -> {after}"
    );

    for _race in 0..RACES {
        now += 1;
        scene
            .index
            .mark(&scene.fleet)
            .await
            .expect("the peeked generation");
        let (polled, sent) = tokio::join!(scene.poll(now), scene.send());
        let delivered = if let Some(acquired) = polled {
            acquired
        } else {
            assert!(
                scene.marked().await,
                "a mark written during the poll was cleared by it, stranding the event"
            );
            now += 1;
            scene
                .poll(now)
                .await
                .expect("the event is leased on the next poll")
        };
        assert_eq!(
            delivered.event_id, sent,
            "the event ingress sent is the one leased"
        );
        scene.finish(&delivered, now).await;
        now += 1;
        assert!(scene.poll(now).await.is_none(), "the finished fleet drains");
        assert!(!scene.marked().await, "and its mark is cleared");
    }

    scene.cleanup().await;
}

/// An entry another replica read and never leased is delivered by the next
/// won claim, and the mark goes only once the fleet is drained.
///
/// The lease used to read only its own consumer's pending list, which cannot
/// see this entry; a poll that then found nothing new would call the fleet
/// drained and clear its mark over work no process reads.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_entry_pending_elsewhere_is_delivered() {
    let scene = scene().await;
    let now = ENROLLED_AT;
    let event_id = scene.send().await;
    let stranded = FleetStreams::new(scene.fixtures.queue().clone())
        .read_new(&scene.fleet, DEAD_REPLICA)
        .await
        .expect("the read must not fault")
        .expect("the dead replica was handed the entry");

    let delivered = scene
        .poll(now)
        .await
        .expect("a won claim takes over what another consumer left pending");
    assert_eq!(delivered.event_id, event_id);
    assert_eq!(
        delivered.receipt, stranded.receipt,
        "the same entry, not a copy"
    );
    assert!(
        scene.marked().await,
        "a fleet with work in flight keeps its mark"
    );

    scene.finish(&delivered, now).await;
    assert!(
        scene.poll(now + 1).await.is_none(),
        "the finished fleet drains"
    );
    assert!(!scene.marked().await, "only then is its mark cleared");

    scene.cleanup().await;
}

/// A slot a live runner holds is not a candidate, so another runner's poll
/// spends no claim finding that out — and the fleet keeps its mark.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_candidates_skip_held_slots() {
    let scene = scene().await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let [holder, other] = &scene.runners;
    scene.send().await;
    scene
        .leases
        .claim(&scene.fleet_id, holder, now, LEASE_TTL_MS)
        .await
        .expect("the claim must not fault")
        .expect("an unclaimed slot is winnable");

    let mut reached = None;
    for _poll in 0..READY_PARTITIONS {
        let (selected, cost) = scene.leases.select_measured(other, now).await;
        let selected = selected.expect("the poll must not fault");
        assert!(
            selected.is_none(),
            "a held slot was handed to a second runner"
        );
        if cost.candidates_scanned > 0 {
            reached = Some(cost);
        }
    }
    let cost = reached.expect("one poll in the rotation reaches the fleet's partition");
    assert_eq!(
        cost.database_roundtrips, 1,
        "the candidate query skips the held slot, so no claim is spent losing it"
    );
    assert!(
        scene.marked().await,
        "a held fleet keeps its mark: its work is in flight, not drained"
    );

    scene.cleanup().await;
}
