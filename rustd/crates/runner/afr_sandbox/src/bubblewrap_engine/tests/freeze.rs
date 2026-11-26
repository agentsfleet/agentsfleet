//! Parking and resuming a prepared sandbox: the engine hands both to its
//! cgroup's freezer, off the async runtime, and reads quiescence from the
//! tenant leaf.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::fs;
use std::os::unix::fs::MetadataExt as _;

use super::support::{FakeHost, SLEEPER, id_of, serve_runs, spec};
use crate::bubblewrap_engine::names::Names;
use crate::engine::{Engine, ParkMode, Quiescence};
use crate::error::EgressRefusal;
use crate::network::{Allowlist, Network};

/// The run the test prepares.
const RUN: &str = "run-21";
/// Where the kernel reports a settled freeze. The fake host's cgroup is a
/// plain directory, so the test writes what the kernel would.
const EVENTS: &str = "cgroup.events";
/// What the freezer writes to stop or start the tree.
const FREEZE: &str = "cgroup.freeze";

#[tokio::test]
async fn test_a_prepared_sandbox_parks_and_resumes_through_its_cgroup() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_runs(host.config.state_dir.clone());
    let id = id_of(RUN);
    let sandbox = host.engine().prepare(spec(&id)).await.unwrap();
    let run_cgroup = host.config.cgroup_root.join(id.as_str());

    fs::write(run_cgroup.join("tenant").join(EVENTS), "populated 1\n").unwrap();
    let busy = sandbox.quiesce().await.unwrap();
    fs::write(run_cgroup.join("tenant").join(EVENTS), "populated 0\n").unwrap();
    let quiet = sandbox.quiesce().await.unwrap();
    fs::write(run_cgroup.join(EVENTS), "populated 1\nfrozen 1\n").unwrap();
    let parked = sandbox.park(ParkMode::Pause).await.unwrap();
    let asked_to_freeze = fs::read_to_string(run_cgroup.join(FREEZE)).unwrap();
    fs::write(run_cgroup.join(EVENTS), "populated 1\nfrozen 0\n").unwrap();
    let resumed = parked.resume().await.unwrap();
    let asked_to_thaw = fs::read_to_string(run_cgroup.join(FREEZE)).unwrap();
    let _left = resumed.destroy().await;
    server.abort();

    assert_eq!(
        (busy, quiet),
        (Quiescence::Busy, Quiescence::Quiet),
        "a detached tenant process is busy; the resident executor alone is quiet"
    );
    assert_eq!(
        (asked_to_freeze.as_str(), asked_to_thaw.as_str()),
        ("1", "0"),
        "the park and the resume each reach the sandbox's own cgroup"
    );
}

/// Bubblewrap parks only by pausing: a stop is refused naming `park`, and
/// the sandbox is destroyed rather than kept.
#[tokio::test]
async fn test_a_park_mode_bubblewrap_lacks_is_refused_and_destroys() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_runs(host.config.state_dir.clone());
    let id = id_of(RUN);
    let sandbox = host.engine().prepare(spec(&id)).await.unwrap();

    let refused = sandbox.park(ParkMode::Stop).await.unwrap_err();
    server.abort();

    assert_eq!(refused.detail(), "the bubblewrap engine cannot park");
    assert!(!host.run_dir(RUN).exists(), "destroyed, not kept");
}

/// A sandbox built to no allowlist has no addresses to replace: asking it to
/// take an allowlist is refused, never taken as done.
#[tokio::test]
async fn test_a_sandbox_built_to_no_allowlist_refuses_new_addresses() {
    let host = FakeHost::new(SLEEPER);
    let server = serve_runs(host.config.state_dir.clone());
    let id = id_of(RUN);
    let mut sandbox = host.engine().prepare(spec(&id)).await.unwrap();

    let refused = sandbox.reallow(&Allowlist::new(Vec::new()).unwrap()).await;
    let _left = sandbox.destroy().await;
    server.abort();

    assert_eq!(
        refused.unwrap_err().egress_refusal(),
        Some(&EgressRefusal::NoScope)
    );
}

/// A resumed sandbox's names are rewritten in the file its bind already holds:
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
    let before = fs::metadata(&hosts).unwrap().ino();

    Names::rewrite(dir.path(), &moved).unwrap();

    assert_eq!(fs::metadata(&hosts).unwrap().ino(), before);
    assert_eq!(fs::read_to_string(&hosts).unwrap(), moved.hosts_file());
}
