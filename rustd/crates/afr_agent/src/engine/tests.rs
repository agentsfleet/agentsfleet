use afd_wire::activity::ActivityFrame;
use afr_egress::testing::CountingMint;
use tokio_util::sync::CancellationToken;

use super::{AgentRun, Meter};
use crate::fixture::{API_KEY, GITHUB_TOKEN, lease, unbounded};
use crate::testing::Discard;

#[test]
fn a_run_debugs_without_the_leases_secrets() {
    let lease = lease(&[], unbounded());
    let sink = |_frame: ActivityFrame<'static>| {};
    let run = AgentRun {
        lease: &lease,
        memory: afr_memory::Seed::default(),
        executor: None,
        mint: &CountingMint::never(),
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        events: &sink,
        meter: &Meter::default(),
        stop: &CancellationToken::new(),
    };

    let rendered = format!("{run:?}");

    assert!(rendered.contains(&*lease.lease_id), "{rendered}");
    for secret in [API_KEY, GITHUB_TOKEN] {
        assert!(
            !rendered.contains(secret),
            "a lease secret must never render: {rendered}"
        );
    }
}
