#![expect(
    clippy::unwrap_used,
    reason = "test module: a precondition that fails should fail the test loudly"
)]

use std::fs;
use std::os::fd::AsFd as _;

use afd_core::test_util::trace::Capture;

use super::kernel::{Host, Kernel as _};
use super::slot::{Claim, Slot, claims_held};
use super::testing::{DELLINK, DELTABLE, Fake, NEWTABLE, Protocol};
use super::{enforceable, namespace_of, probe, sweep};

/// The probe asks forwarding first: a host that forwards nothing could build
/// a scope and still carry no packet through it.
#[test]
fn test_the_probe_needs_forwarding() {
    let dir = tempfile::tempdir().unwrap();
    let off = dir.path().join("off");
    fs::write(&off, "0\n").unwrap();

    let refused = probe(&Fake::default(), &off).unwrap_err();
    let unreadable = probe(&Fake::default(), &dir.path().join("absent"));

    assert!(refused.to_string().contains("ip_forward"), "{refused}");
    unreadable.unwrap_err();
}

/// With forwarding on, the probe builds a whole scope and removes it, from
/// inside a namespace made for it, so the host's own is never touched.
#[test]
fn test_the_probe_builds_and_removes_a_scope_of_its_own() {
    let _claims = claims_held();
    let dir = tempfile::tempdir().unwrap();
    let on = dir.path().join("on");
    fs::write(&on, "1\n").unwrap();
    let kernel = Fake::default();
    let failing = Fake::default().refusing(Protocol::Netfilter, NEWTABLE, libc::EOPNOTSUPP);

    probe(&kernel, &on).unwrap();
    let refused = probe(&failing, &on).unwrap_err();

    assert_eq!(
        kernel.entered(),
        2,
        "the probe's namespace, then the sandbox's"
    );
    assert!(kernel.seen_on(Protocol::Route).contains(&DELLINK));
    assert!(kernel.seen_on(Protocol::Netfilter).contains(&DELTABLE));
    assert!(
        refused.to_string().contains("the egress table"),
        "{refused}"
    );
}

/// Without the privilege to make a namespace, this host cannot hold a sandbox
/// to an allowlist, and says why; as root, the kernel lane proves the rest.
#[test]
fn test_an_unprivileged_probe_reports_no_enforcement() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let capture = Capture::install();

    assert!(!enforceable());
    assert!(
        capture
            .only("egress_probe_failed")
            .field("reason")
            .is_some()
    );
}

/// A sandbox none of whose processes runs in a namespace of its own is
/// refused rather than joined: joining the host's own namespace to itself
/// would hold nothing.
#[test]
fn test_a_sandbox_sharing_the_hosts_namespace_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let procs = dir.path().join("cgroup.procs");
    fs::write(
        &procs,
        format!("{}\nnot-a-pid\n4194304\n", std::process::id()),
    )
    .unwrap();

    let refused = namespace_of(&procs).unwrap_err();
    let unreadable = namespace_of(&dir.path().join("absent"));

    assert!(
        refused.to_string().contains("namespace of its own"),
        "{refused}"
    );
    unreadable.unwrap_err();
}

/// The boot sweep removes what carries this crate's names and nothing else,
/// and leaves alone a slot a scope in this process still holds.
#[test]
fn test_the_sweep_removes_only_leftovers() {
    let _claims = claims_held();
    let capture = Capture::install();
    let live = Claim::exactly(Slot::new(9).unwrap()).unwrap();
    let kernel = Fake::default().holding(
        &["afegress3", "afegress07", "filter", "afegress9"],
        &["lo", "afv3", "afv9", "eth0"],
    );

    sweep(&kernel).unwrap();

    drop(live);
    assert_eq!(
        kernel
            .seen_on(Protocol::Route)
            .iter()
            .filter(|&&step| step == DELLINK)
            .count(),
        1
    );
    assert_eq!(
        kernel
            .seen_on(Protocol::Netfilter)
            .iter()
            .filter(|&&step| step == DELTABLE)
            .count(),
        1
    );
    assert_eq!(capture.only("egress_swept").field("slot"), Some("3"));
}

/// A sweep that cannot list fails; a leftover it lists but cannot remove is
/// logged and its slot kept, so no scope in this run meets it.
#[test]
fn test_a_sweep_says_what_it_could_not_do() {
    let _claims = claims_held();
    let capture = Capture::install();
    let blind = Fake::default().closed(Protocol::Netfilter);
    let stuck = Fake::default().holding(&["afegress6"], &[]).refusing(
        Protocol::Netfilter,
        DELTABLE,
        libc::EBUSY,
    );

    let refused = sweep(&blind).unwrap_err();
    sweep(&stuck).unwrap();

    assert!(
        refused.to_string().contains("listing egress tables"),
        "{refused}"
    );
    assert_eq!(capture.only("egress_sweep_failed").field("slot"), Some("6"));
    assert!(Claim::exactly(Slot::new(6).unwrap()).is_none());
}

/// The running kernel's sockets open without privilege; making or joining a
/// namespace needs it, and fails cleanly without.
#[test]
fn test_the_host_kernel_opens_its_sockets() {
    let own = fs::File::open("/proc/self/ns/net").unwrap();
    let root = rustix::process::geteuid().is_root();

    Host.route().unwrap();
    Host.netfilter().unwrap();
    let fresh = Host.fresh_namespace();
    let joined = Host.inside(own.as_fd(), || Ok::<_, std::io::Error>(()));

    assert_eq!(fresh.is_ok(), root);
    assert_eq!(joined.is_ok(), root);
}
