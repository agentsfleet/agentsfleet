//! A held sandbox whose allowlisted name resolved to a new address since its
//! lease parked it: the hold is kept, and the sandbox takes the new address
//! while still frozen, or is given up for a fresh one when it will not.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::{IpAddr, Ipv4Addr};

use afd_core::test_util::trace::Capture;
use afd_wire::runner::NetworkPolicy;
use tokio::sync::mpsc;

use super::EVENT_REFILL_FAILED;
use super::tests::{Counted, NEXT_LEASE_ID, released, releases, resumed, settled};
use crate::holds::Release;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, FakeResolver, Freezer, LEASE_ID, NO_REFILL, Rig,
    assigned, daemon, lease,
};

/// The registry the fleet's leases reach, and where it resolved each time.
const MIRROR: &str = "mirror.example";
const FIRST: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);
const MOVED: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 7);

/// A rig with room for holds over `engine`, whose resolver answers the
/// mirror at its first address, then at the one it moved to.
fn moving(engine: FakeEngine) -> (Rig, Counted) {
    let counted = Counted::of(&engine);
    let resolver =
        FakeResolver::moving(&[(MIRROR, vec![&[IpAddr::V4(FIRST)], &[IpAddr::V4(MOVED)]])]);
    let rig = Rig::resolving(
        daemon(|_call| None),
        engine,
        FakeAgent::new(Behaviour::Answer),
        resolver,
    );
    rig.lessee.holds.resize(2);
    (rig, counted)
}

/// Runs a lease that parks under the mirror, then the fleet's next lease,
/// told to resume, once the mirror has moved.
async fn park_then_resume(rig: &Rig) {
    let egress = assigned(NetworkPolicy::AllowListEgress, &[MIRROR]);
    rig.run_under(&lease(LEASE_ID, FLEET_ID, None), &egress)
        .await
        .unwrap();
    rig.run_under(&resumed(NEXT_LEASE_ID, FLEET_ID), &egress)
        .await
        .unwrap();
}

/// The name moved, so the hold is kept: the held sandbox is told the mirror's
/// new address while still frozen, then thawed, and serves the lease. No
/// sandbox is built fresh.
#[tokio::test(start_paused = true)]
async fn a_name_that_moved_keeps_the_hold_and_takes_its_new_address() {
    let (told, mut reallowed) = mpsc::unbounded_channel();
    let engine = FakeEngine {
        reallowed: Some(told),
        ..FakeEngine::default()
    };
    let (rig, counted) = moving(engine);

    park_then_resume(&rig).await;

    assert_eq!(
        counted.read(),
        (1, 2, 1, 0),
        "one sandbox, refilled and held"
    );
    let (allowlist, thawed) = reallowed.recv().await.unwrap();
    assert_eq!(allowlist.addresses(), [MOVED]);
    assert!(!thawed, "the addresses change while it is frozen");
    assert!(reallowed.try_recv().is_err(), "the first lease built fresh");
}

/// A held sandbox that will not take the new address is given up: the log
/// names the refill and its reason, the hold ends as a failed resume, and the
/// lease runs in a fresh sandbox built to the new address.
#[tokio::test(start_paused = true)]
async fn a_hold_that_refuses_its_new_address_falls_back_fresh() {
    let capture = Capture::install();
    let engine = FakeEngine {
        freezer: Freezer::RefusesReallow,
        ..FakeEngine::default()
    };
    let (rig, counted) = moving(engine);

    park_then_resume(&rig).await;
    settled(&rig).await;

    assert_eq!(
        counted.read(),
        (2, 2, 0, 2),
        "never thawed, then built fresh"
    );
    let refused = capture.only(EVENT_REFILL_FAILED);
    assert!(
        refused
            .field("reason")
            .is_some_and(|reason| reason.contains(NO_REFILL)),
        "{refused:?}"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::ThawFailed),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}
