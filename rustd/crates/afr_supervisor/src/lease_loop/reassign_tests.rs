//! A reassignment between two of a fleet's leases: the hold the first lease
//! left was built to reach what the runner was assigned then, so the next
//! lease, under another egress, builds fresh and the hold ends.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::{IpAddr, Ipv4Addr};

use afd_core::test_util::trace::Capture;
use afd_wire::runner::NetworkPolicy;

use super::tests::{Counted, NEXT_LEASE_ID, released, releases, resumed, settled};
use crate::egress::Egress;
use crate::holds::Release;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, FakeResolver, LEASE_ID, Rig, assigned, daemon,
    lease,
};

/// Two registries the resolver answers, at different addresses.
const MIRROR: &str = "mirror.example";
const OTHER_MIRROR: &str = "other-mirror.example";

/// A rig with room for holds, resolving both registries.
fn holding() -> (Rig, Counted) {
    let engine = FakeEngine::default();
    let counted = Counted::of(&engine);
    let resolver = FakeResolver::answering(&[
        (MIRROR, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))]),
        (OTHER_MIRROR, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2))]),
    ]);
    let rig = Rig::resolving(
        daemon(|_call| None),
        engine,
        FakeAgent::new(Behaviour::Answer),
        resolver,
    );
    rig.lessee.holds.resize(2);
    (rig, counted)
}

/// The fleet's next lease is told to resume, but the runner's egress changed
/// since its hold was built: from the host's network to none, and from one
/// allowlist to another. Each time the lease builds fresh and the hold ends as
/// a mismatch, never serving it.
#[tokio::test(start_paused = true)]
async fn test_reassignment_retires_held_sandboxes() {
    let reassignments: [(Egress, Egress); 2] = [
        (
            assigned(NetworkPolicy::AllowAll, &[]),
            assigned(NetworkPolicy::DenyAllEgress, &[]),
        ),
        (
            assigned(NetworkPolicy::AllowListEgress, &[MIRROR]),
            assigned(NetworkPolicy::AllowListEgress, &[OTHER_MIRROR]),
        ),
    ];

    for (before, after) in reassignments {
        let capture = Capture::install();
        let (rig, counted) = holding();

        rig.run_under(&lease(LEASE_ID, FLEET_ID, None), &before)
            .await
            .unwrap();
        rig.run_under(&resumed(NEXT_LEASE_ID, FLEET_ID), &after)
            .await
            .unwrap();
        settled(&rig).await;

        assert_eq!(
            counted.read(),
            (2, 2, 0, 2),
            "built fresh, never thawed: {before:?} then {after:?}"
        );
        assert_eq!(
            releases(&capture),
            [
                released(FLEET_ID, Release::Mismatch),
                released(FLEET_ID, Release::Shutdown)
            ]
        );
    }
}

/// The same egress at the next lease keeps the hold: it is thawed and serves.
#[tokio::test(start_paused = true)]
async fn an_unchanged_egress_keeps_the_hold() {
    let (rig, counted) = holding();
    let egress = assigned(NetworkPolicy::AllowListEgress, &[MIRROR]);

    rig.run_under(&lease(LEASE_ID, FLEET_ID, None), &egress)
        .await
        .unwrap();
    rig.run_under(&resumed(NEXT_LEASE_ID, FLEET_ID), &egress)
        .await
        .unwrap();

    assert_eq!(counted.read(), (1, 2, 1, 0), "one sandbox, thawed and held");
}
