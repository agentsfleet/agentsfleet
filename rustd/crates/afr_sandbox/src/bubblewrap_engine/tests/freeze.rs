//! Freezing and thawing a prepared sandbox: the engine hands both to its
//! lease cgroup's freezer, off the async runtime.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;
use std::os::unix::fs::MetadataExt as _;

use super::support::{FakeHost, SLEEPER, request, serve_leases};
use crate::bubblewrap_engine::names::Names;
use crate::engine::Engine;
use crate::error::EgressRefusal;
use crate::network::{Allowlist, Network};

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

/// A sandbox built to no allowlist has no addresses to replace: asking it to
/// take an allowlist is refused, never taken as done.
#[tokio::test]
async fn test_a_sandbox_built_to_no_allowlist_refuses_new_addresses() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_leases(host.config.state_dir.clone());
    let mut sandbox = host.engine().prepare(request(LEASE)).await.unwrap();

    let refused = sandbox.reallow(&Allowlist::new(Vec::new()).unwrap()).await;
    let _left = sandbox.destroy().await;
    server.abort();

    assert_eq!(
        refused.unwrap_err().egress_refusal(),
        Some(&EgressRefusal::NoScope)
    );
}

/// A held sandbox's names are rewritten in the file its bind already holds:
/// the same file, now naming the new addresses.
#[test]
fn test_rewritten_names_land_in_the_file_the_sandbox_reads() {
    let dir = tempfile::tempdir().unwrap();
    let name = "a.example".to_owned();
    let first = Allowlist::new(vec![(name.clone(), [10, 0, 0, 1].into())]).unwrap();
    let moved = Allowlist::new(vec![(name, [10, 0, 0, 2].into())]).unwrap();
    let Names::Rendered { hosts, .. } =
        Names::render(dir.path(), Network::Allowed(&first)).unwrap()
    else {
        unreachable!("an allowlist renders its names")
    };
    let before = std::fs::metadata(&hosts).unwrap().ino();

    Names::rewrite(dir.path(), &moved).unwrap();

    assert_eq!(std::fs::metadata(&hosts).unwrap().ino(), before);
    assert_eq!(std::fs::read_to_string(&hosts).unwrap(), moved.hosts_file());
}
