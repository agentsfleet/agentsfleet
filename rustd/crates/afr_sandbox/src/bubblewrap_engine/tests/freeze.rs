//! Freezing and thawing a prepared sandbox: the engine hands both to its
//! lease cgroup's freezer, off the async runtime.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;

use super::support::{FakeHost, SLEEPER, request, serve_leases};
use crate::engine::Engine;

/// The lease the test prepares.
const LEASE: &str = "lease-21";
/// Where the kernel reports a settled freeze. The fake host's cgroup is a
/// plain directory, so the test writes what the kernel would.
const EVENTS: &str = "cgroup.events";
/// What the freezer writes to stop or start the tree.
const FREEZE: &str = "cgroup.freeze";

#[tokio::test]
async fn test_a_prepared_sandbox_freezes_and_thaws_through_its_lease_cgroup() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let sandbox = host.engine().prepare(request(LEASE)).await.unwrap();
    let lease_cgroup = host.config.cgroup_root.join(LEASE);

    fs::write(lease_cgroup.join(EVENTS), "populated 1\nfrozen 1\n").unwrap();
    let frozen = sandbox.freeze().await;
    let asked_to_freeze = fs::read_to_string(lease_cgroup.join(FREEZE)).unwrap();
    fs::write(lease_cgroup.join(EVENTS), "populated 1\nfrozen 0\n").unwrap();
    let thawed = sandbox.thaw().await;
    let asked_to_thaw = fs::read_to_string(lease_cgroup.join(FREEZE)).unwrap();
    let _left = sandbox.destroy().await;
    server.abort();

    frozen.unwrap();
    thawed.unwrap();
    assert_eq!(
        (asked_to_freeze.as_str(), asked_to_thaw.as_str()),
        ("1", "0"),
        "the freeze and the thaw each reach the lease's own cgroup"
    );
}
