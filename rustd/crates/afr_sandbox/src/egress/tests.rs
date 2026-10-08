#![expect(
    clippy::unwrap_used,
    reason = "test module: a precondition that fails should fail the test loudly"
)]

use std::fs;
use std::os::fd::AsFd as _;

use afd_core::test_util::trace::Capture;
use netlink_packet_netfilter::NetfilterProtoFamily;

use super::kernel::{Host, Kernel as _};
use super::slot::{Claim, Slot, claims_held};
use super::testing::{DELLINK, DELTABLE, Fake, GETCHAIN, NEWTABLE, Protocol};
use super::{enforceable, namespace_of, probe, sweep};
use crate::error::{EgressRefusal, Step};

/// A file reading as forwarding on, in `dir`.
fn forwarding_on(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let on = dir.path().join("on");
    fs::write(&on, "1\n").unwrap();
    on
}

/// The probe asks forwarding first: a host that forwards nothing could build
/// a scope and still carry no packet through it.
#[test]
fn test_the_probe_needs_forwarding() {
    let dir = tempfile::tempdir().unwrap();
    let off = dir.path().join("off");
    fs::write(&off, "0\n").unwrap();

    let refused = probe(&Fake::default(), &off).unwrap_err();
    let unreadable = probe(&Fake::default(), &dir.path().join("absent"));

    assert_eq!(
        refused.egress_refusal(),
        Some(&EgressRefusal::ForwardingOff)
    );
    unreadable.unwrap_err();
}

/// With forwarding on, the probe builds a whole scope and removes it, from
/// inside a namespace made for it, so the host's own is never touched.
#[test]
fn test_the_probe_builds_and_removes_a_scope_of_its_own() {
    let _claims = claims_held();
    let dir = tempfile::tempdir().unwrap();
    let on = forwarding_on(&dir);
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
    assert_eq!(refused.netlink_step(), Some(Step::InstallRules));
}

/// A host whose own forward chain drops by policy would pass no allowlisted
/// connection, whatever the sandbox's table admits: the probe refuses, names
/// each such chain, and builds nothing. A host whose chains cannot be listed
/// is refused too, naming the step.
#[test]
fn test_the_probe_refuses_a_host_whose_forward_chain_drops() {
    let _claims = claims_held();
    let dir = tempfile::tempdir().unwrap();
    let on = forwarding_on(&dir);
    let dropping = Fake::default()
        .dropping_forward(NetfilterProtoFamily::IPv4, "filter", "FORWARD")
        .dropping_forward(NetfilterProtoFamily::Inet, "ufw", "forward");
    let unlisted = Fake::default().refusing(Protocol::Netfilter, GETCHAIN, libc::EPERM);

    let refused = probe(&dropping, &on).unwrap_err();
    let blind = probe(&unlisted, &on).unwrap_err();

    let chains = ["ip filter FORWARD", "inet ufw forward"].map(str::to_owned);
    assert_eq!(
        refused.egress_refusal(),
        Some(&EgressRefusal::ForwardDropped(chains.to_vec()))
    );
    assert_eq!(dropping.entered(), 0, "no scope is built");
    assert!(
        !dropping.seen_on(Protocol::Netfilter).contains(&NEWTABLE),
        "the host's chains are only read"
    );
    assert_eq!(blind.netlink_step(), Some(Step::ListChains));
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

    assert_eq!(refused.egress_refusal(), Some(&EgressRefusal::NoNamespace));
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

    assert_eq!(refused.netlink_step(), Some(Step::ListTables));
    assert_eq!(capture.only("egress_sweep_failed").field("slot"), Some("6"));
    assert!(Claim::exactly(Slot::new(6).unwrap()).is_none());
}

/// A sweep that lists the tables but cannot list the links fails naming that
/// listing, before it removes anything or claims any slot.
#[test]
fn test_a_sweep_that_cannot_list_the_links_removes_nothing() {
    let _claims = claims_held();
    let kernel = Fake::default()
        .holding(&["afegress4"], &["afv4"])
        .closed(Protocol::Route);

    let refused = sweep(&kernel).unwrap_err();

    assert_eq!(refused.netlink_step(), Some(Step::ListLinks));
    assert!(
        !kernel.seen_on(Protocol::Netfilter).contains(&DELTABLE),
        "{:?}",
        kernel.seen()
    );
    assert!(
        Claim::exactly(Slot::new(4).unwrap()).is_some(),
        "the slot is free"
    );
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
