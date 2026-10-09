#![expect(
    clippy::unwrap_used,
    reason = "test module: a precondition that fails should fail the test loudly"
)]

use std::fs::File;
use std::net::Ipv4Addr;
use std::os::fd::AsFd as _;

use afd_core::test_util::trace::Capture;

use super::{Scope, remove};
use crate::egress::slot::{Claim, Slot, claims_held};
use crate::egress::testing::{
    DELLINK, DELSETELEM, DELTABLE, Fake, GETLINK, NEWADDR, NEWCHAIN, NEWLINK, NEWROUTE, NEWRULE,
    NEWSET, NEWSETELEM, NEWTABLE, Protocol, SETLINK,
};
use crate::error::{EgressRefusal, Step};
use crate::network::Allowlist;

/// A name the allowlist carries two addresses for, as a round-robin DNS
/// answer gives them.
const TWICE_RESOLVED: &str = "b.example";

fn allowlist() -> Allowlist {
    Allowlist::new(vec![
        ("a.example".to_owned(), Ipv4Addr::new(10, 0, 0, 1)),
        (TWICE_RESOLVED.to_owned(), Ipv4Addr::new(10, 0, 0, 2)),
        (TWICE_RESOLVED.to_owned(), Ipv4Addr::new(10, 0, 0, 3)),
    ])
    .unwrap()
}

fn namespace() -> File {
    File::open("/dev/null").unwrap()
}

/// The slot a logged event names.
fn slot_in(capture: &Capture, event: &str) -> Slot {
    let index = capture.only(event).field("slot").unwrap().parse().unwrap();
    Slot::new(index).unwrap()
}

/// The table is built before the link exists, so the link never carries a
/// packet its rules have not seen; the peer is configured inside the sandbox's
/// namespace. Removal takes the link, then the table, and frees the slot.
#[test]
fn test_a_scope_builds_its_rules_before_its_link() {
    let _claims = claims_held();
    let capture = Capture::install();
    let kernel = Fake::default();

    let scope = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap();
    let slot = slot_in(&capture, "egress_scope_built");
    scope.remove(&kernel).unwrap();

    let netfilter = (Protocol::Netfilter, NEWTABLE);
    let built: Vec<_> = kernel.seen();
    assert_eq!(built.first(), Some(&netfilter));
    let first_link = built
        .iter()
        .position(|step| *step == (Protocol::Route, NEWLINK))
        .unwrap();
    let before_link = built.get(..first_link).unwrap_or_default();
    assert!(
        before_link
            .iter()
            .all(|(protocol, _)| *protocol == Protocol::Netfilter)
    );
    assert_eq!(
        kernel.seen_on(Protocol::Netfilter),
        [
            [NEWTABLE, NEWSET, NEWCHAIN, NEWCHAIN, NEWCHAIN, NEWSETELEM].as_slice(),
            &[NEWRULE; 8],
            &[DELTABLE],
        ]
        .concat()
    );
    assert_eq!(
        kernel.seen_on(Protocol::Route),
        [
            NEWLINK, GETLINK, NEWADDR, SETLINK, GETLINK, NEWADDR, SETLINK, NEWROUTE, DELLINK
        ]
    );
    assert_eq!(kernel.entered(), 1, "the peer is configured where it lives");
    assert_eq!(capture.only("egress_scope_built").field("hosts"), Some("2"));
    assert!(Claim::exactly(slot).is_some(), "the slot is free again");
}

/// Every netlink step that fails refuses the lease, naming the step; what was
/// built comes down, and the slot is freed when it did. The log line counts
/// the names and gives neither them nor their addresses.
#[test]
fn test_egress_setup_failures_refuse_the_lease() {
    let _claims = claims_held();
    let cases = [
        // With no netfilter socket, nothing was sent, so the slot is freed.
        (
            Fake::default().closed(Protocol::Netfilter),
            Step::OpenNetfilter,
            true,
        ),
        (
            Fake::default().refusing(Protocol::Netfilter, NEWTABLE, libc::EPERM),
            Step::InstallRules,
            true,
        ),
        (
            Fake::default().refusing(Protocol::Route, NEWLINK, libc::EEXIST),
            Step::Join,
            true,
        ),
        (
            Fake::default().refusing(Protocol::Route, NEWROUTE, libc::ENETUNREACH),
            Step::ConfigurePeer,
            true,
        ),
        // With no route socket, the table is already built and its link
        // cannot be confirmed gone: the slot stays held for the next run's
        // sweep.
        (
            Fake::default().closed(Protocol::Route),
            Step::OpenRoute,
            false,
        ),
    ];
    for (kernel, step, freed) in cases {
        let capture = Capture::install();

        let refused = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap_err();

        assert_eq!(refused.netlink_step(), Some(step));
        let line = capture.only("egress_scope_refused");
        assert_eq!(line.field("hosts"), Some("2"), "{step:?}");
        assert!(
            !format!("{:?}", line.fields).contains("example")
                && !format!("{:?}", line.fields).contains("10.0.0"),
            "{step:?}: no names or addresses are logged"
        );
        let slot = slot_in(&capture, "egress_scope_refused");
        assert_eq!(Claim::exactly(slot).is_some(), freed, "{step:?}");
    }
}

/// A scope whose link will not come down keeps its slot for the process's
/// life, so no later scope meets the link it left.
#[test]
fn test_a_scope_that_will_not_come_down_keeps_its_slot() {
    let _claims = claims_held();
    let capture = Capture::install();
    let kernel = Fake::default().refusing(Protocol::Route, DELLINK, libc::EBUSY);
    let scope = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap();

    let left = scope.remove(&kernel).unwrap_err();

    assert_eq!(left.netlink_step(), Some(Step::RemoveLink));
    assert!(
        kernel.seen_on(Protocol::Netfilter).contains(&DELTABLE),
        "the table still goes"
    );
    assert!(Claim::exactly(slot_in(&capture, "egress_scope_left")).is_none());
}

/// A host whose every slot is held refuses the next scope before asking the
/// kernel for anything.
#[test]
fn test_a_host_with_every_slot_held_refuses() {
    let _claims = claims_held();
    let capture = Capture::install();
    let kernel = Fake::default();
    let held: Vec<Claim> = std::iter::from_fn(Claim::any).collect();

    let refused = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap_err();

    drop(held);
    assert_eq!(refused.egress_refusal(), Some(&EgressRefusal::NoSlot));
    let line = capture.only("egress_scope_refused");
    assert_eq!(line.field("slot"), None);
    assert_eq!(line.field("refusal"), Some(EgressRefusal::NoSlot.as_str()));
    assert_eq!(kernel.seen(), [], "the kernel is asked nothing");
}

/// A leftover's link goes even when no `nf_tables` socket opens to remove its
/// table, and the refusal names the table's removal.
#[test]
fn test_a_leftovers_link_goes_even_when_its_table_cannot() {
    let kernel = Fake::default().closed(Protocol::Netfilter);
    let slot = Slot::new(9).unwrap();

    let refused = remove(&kernel, slot).unwrap_err();

    assert_eq!(kernel.seen_on(Protocol::Route), [DELLINK]);
    assert_eq!(refused.netlink_step(), Some(Step::RemoveRules));
}

/// A built scope takes a new allowlist in place: its set is emptied and
/// refilled through the socket that owns its table, and the log counts the
/// names it now admits. A swap the kernel refuses names its step.
#[test]
fn test_a_scope_takes_new_addresses_in_place() {
    let _claims = claims_held();
    let capture = Capture::install();
    let kernel = Fake::default();
    let refusing = Fake::default().refusing(Protocol::Netfilter, DELSETELEM, libc::EPERM);
    let mut scope = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap();
    let mut stuck = Scope::build(&refusing, namespace().as_fd(), &allowlist()).unwrap();
    let moved = Allowlist::new(vec![(
        TWICE_RESOLVED.to_owned(),
        Ipv4Addr::new(10, 0, 0, 9),
    )])
    .unwrap();

    scope.reallow(&moved).unwrap();
    let refused = stuck.reallow(&moved).unwrap_err();

    assert!(
        kernel
            .seen_on(Protocol::Netfilter)
            .ends_with(&[DELSETELEM, NEWSETELEM]),
        "{:?}",
        kernel.seen_on(Protocol::Netfilter)
    );
    assert_eq!(refused.netlink_step(), Some(Step::RefillRules));
    assert_eq!(
        capture.only("egress_scope_refilled").field("hosts"),
        Some("1")
    );
    scope.remove(&kernel).unwrap();
    stuck.remove(&refusing).unwrap();
}

/// A refill the kernel refuses part-way leaves its later replies on the
/// scope's socket, and the removal after it reads only its own: a leftover
/// refusal does not strand a slot whose table came down, and a leftover
/// acknowledgement does not hide the removal's own refusal.
#[test]
fn test_a_removal_after_a_refused_refill_reads_its_own_answer() {
    let _claims = claims_held();
    let cases = [
        // The refill's second message is refused too; the table still goes.
        (libc::EEXIST, None),
        // The refill's second message is acknowledged; the table will not go.
        (0, Some(libc::EBUSY)),
    ];
    for (added, removed) in cases {
        let capture = Capture::install();
        let kernel = Fake::default();
        let mut scope = Scope::build(&kernel, namespace().as_fd(), &allowlist()).unwrap();
        let slot = slot_in(&capture, "egress_scope_built");
        let kernel = kernel.refusing(Protocol::Netfilter, DELSETELEM, libc::EPERM);
        let kernel = match added {
            0 => kernel,
            errno => kernel.refusing(Protocol::Netfilter, NEWSETELEM, errno),
        };
        let kernel = match removed {
            Some(errno) => kernel.refusing(Protocol::Netfilter, DELTABLE, errno),
            None => kernel,
        };

        let refill = scope.reallow(&allowlist()).unwrap_err();
        let removal = scope.remove(&kernel);

        assert_eq!(refill.netlink_step(), Some(Step::RefillRules));
        assert!(
            kernel
                .seen_on(Protocol::Netfilter)
                .ends_with(&[DELSETELEM, NEWSETELEM, DELTABLE]),
            "{added}: the refill's whole batch reached the kernel"
        );
        assert_eq!(
            removal.err().and_then(|left| left.netlink_step()),
            removed.map(|_errno| Step::RemoveRules),
            "{added}"
        );
        assert_eq!(Claim::exactly(slot).is_some(), removed.is_none(), "{added}");
    }
}
