//! A lease's egress at bind: the sandbox is asked for the network the
//! runner's assignment resolved to, and a host that cannot be admitted ends
//! the lease before any sandbox is built.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::Ordering;

use afd_core::test_util::trace::Capture;
use afd_wire::runner::NetworkPolicy;
use afr_sandbox::Allowlist;
use tokio::sync::mpsc;

use super::{DETAIL_EGRESS, DETAIL_EGRESS_BLOCKED, EVENT_EGRESS_REFUSED};
use crate::egress::Bound;
use crate::test_support::{
    Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, FakeResolver, LEASE_ID, Rig,
    STARTUP_POSTURE, assigned, daemon, lease, reported,
};

/// A registry host the resolver answers, and the address it answers with.
const REGISTRY: &str = "registry.example";
const REGISTRY_ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 7);
/// A registry host the resolver has never heard of.
const UNKNOWN: &str = "unknown.example";
/// A host a fleet may name that resolves to the cloud metadata service.
const INSIDE: &str = "inside.example";
const METADATA_ADDRESS: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);

fn rig(engine: FakeEngine) -> Rig {
    let resolver = FakeResolver::answering(&[
        (REGISTRY, &[IpAddr::V4(REGISTRY_ADDRESS)]),
        (INSIDE, &[IpAddr::V4(METADATA_ADDRESS)]),
    ]);
    Rig::resolving(
        daemon(|_call| None),
        engine,
        FakeAgent::new(Behaviour::Answer),
        resolver,
    )
}

/// Under each posture, the sandbox is asked for the network it maps to: the
/// host's, none, or the registry's resolved addresses.
#[tokio::test(start_paused = true)]
async fn the_sandbox_is_asked_for_the_network_the_egress_resolved() {
    let (reached, mut asked) = mpsc::unbounded_channel();
    let rig = rig(FakeEngine {
        reached: Some(reached),
        ..FakeEngine::default()
    });

    for policy in [
        NetworkPolicy::AllowAll,
        NetworkPolicy::DenyAllEgress,
        NetworkPolicy::AllowListEgress,
    ] {
        let egress = assigned(policy, &[REGISTRY]);
        rig.run_under(&lease(LEASE_ID, FLEET_ID, None), &egress)
            .await
            .unwrap();
    }

    let allowed = Allowlist::new(vec![(REGISTRY.to_owned(), REGISTRY_ADDRESS)]).unwrap();
    let expected = [Bound::Host, Bound::Isolated, Bound::Allowed(allowed)];
    for bound in expected {
        assert_eq!(asked.recv().await, Some(bound));
    }
}

/// What a refusal at bind leaves behind: a startup-posture report carrying
/// `detail`, no sandbox built, and one refusal log naming `host` among the two
/// hosts asked for, never `address`.
fn assert_refused_at_bind(
    rig: &mut Rig,
    capture: &Capture,
    detail: &str,
    host: &str,
    address: Ipv4Addr,
) {
    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(report["failure_detail"], detail, "{report}");
    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "no sandbox is built"
    );
    let refused = capture.only(EVENT_EGRESS_REFUSED);
    assert_eq!(refused.field("lease_id"), Some(LEASE_ID));
    assert_eq!(refused.field("detail"), Some(detail));
    assert_eq!(refused.field("hosts"), Some("2"));
    let reason = refused.field("reason").unwrap();
    assert!(reason.contains(host), "{reason}");
    assert!(
        !reason.contains(&address.to_string()),
        "no address is logged: {reason}"
    );
}

/// A host the resolver cannot answer ends the lease at startup with the
/// egress sentence, before any sandbox is built. The log names the reason
/// and how many hosts were asked for, and no address.
#[tokio::test(start_paused = true)]
async fn an_unresolvable_egress_host_ends_the_lease_at_startup() {
    let capture = Capture::install();
    let mut rig = rig(FakeEngine::default());
    let egress = assigned(NetworkPolicy::AllowListEgress, &[REGISTRY, UNKNOWN]);

    rig.run_under(&lease(LEASE_ID, FLEET_ID, None), &egress)
        .await
        .unwrap();

    assert_refused_at_bind(&mut rig, &capture, DETAIL_EGRESS, UNKNOWN, REGISTRY_ADDRESS);
}

/// A host the fleet allows that resolves to an address no fleet may reach
/// ends the lease at startup with its own sentence, which the fleet's owner
/// acts on, before any sandbox is built. The log names the host, never the
/// address.
#[tokio::test(start_paused = true)]
async fn a_fleet_host_at_a_blocked_address_ends_the_lease_at_startup() {
    let capture = Capture::install();
    let mut rig = rig(FakeEngine::default());
    let egress = assigned(NetworkPolicy::AllowListEgress, &[REGISTRY]);
    let mut payload = lease(LEASE_ID, FLEET_ID, None);
    // A read-only fleet keeps its hosts out of the kernel set, so the fixture's
    // `read_only` is turned off to put the host where the check runs.
    payload.policy.network_policy.read_only = false;
    payload.policy.network_policy.allow.push(INSIDE.into());

    rig.run_under(&payload, &egress).await.unwrap();

    assert_refused_at_bind(
        &mut rig,
        &capture,
        DETAIL_EGRESS_BLOCKED,
        INSIDE,
        METADATA_ADDRESS,
    );
}
