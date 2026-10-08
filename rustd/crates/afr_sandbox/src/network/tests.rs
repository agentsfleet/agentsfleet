#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::Ipv4Addr;

use super::{ALLOWLIST_ADDRESSES_MAX, Allowlist, RESOLV_CONF};
use crate::error::EgressRefusal;

fn entry(name: &str, address: [u8; 4]) -> (String, Ipv4Addr) {
    (name.to_owned(), Ipv4Addr::from(address))
}

/// The host-side set holds each address once, in the order the names were
/// merged, however many names share it.
#[test]
fn test_the_set_holds_each_address_once_in_merge_order() {
    let allowlist = Allowlist::new(vec![
        entry("registry.npmjs.org", [104, 16, 0, 35]),
        entry("pypi.org", [151, 101, 0, 223]),
        entry("npm.example", [104, 16, 0, 35]),
    ])
    .unwrap();

    assert_eq!(
        allowlist.addresses(),
        vec![
            Ipv4Addr::new(104, 16, 0, 35),
            Ipv4Addr::new(151, 101, 0, 223)
        ]
    );
}

/// Each name counts once however many addresses it resolved to, and a name
/// sharing an address with another still counts.
#[test]
fn test_each_name_counts_once() {
    let allowlist = Allowlist::new(vec![
        entry("a.example", [10, 0, 0, 2]),
        entry("b.example", [10, 0, 0, 2]),
        entry("a.example", [10, 0, 0, 1]),
    ])
    .unwrap();

    assert_eq!(allowlist.hosts(), 2);
    assert_eq!(allowlist.addresses().len(), 2);
}

/// The sandbox's hosts file names loopback first, then every entry, so a name
/// with two addresses has two lines and `localhost` still resolves.
#[test]
fn test_egress_resolver_files_render() {
    let allowlist = Allowlist::new(vec![
        entry("a.example", [10, 0, 0, 1]),
        entry("b.example", [10, 0, 0, 2]),
        entry("b.example", [10, 0, 0, 3]),
    ])
    .unwrap();

    assert_eq!(
        allowlist.hosts_file(),
        "127.0.0.1 localhost\n::1 localhost\n\
         10.0.0.1 a.example\n10.0.0.2 b.example\n10.0.0.3 b.example\n"
    );
    assert!(
        !RESOLV_CONF
            .lines()
            .any(|line| line.trim_start().starts_with("nameserver")),
        "the resolver file names no server"
    );
}

/// An empty allowlist is a sandbox that reaches nothing by name or address,
/// not a refusal: a fleet may run without the network.
#[test]
fn test_an_empty_allowlist_admits_nothing() {
    let allowlist = Allowlist::new(Vec::new()).unwrap();

    assert_eq!(allowlist.addresses(), Vec::<Ipv4Addr>::new());
    assert_eq!(allowlist.hosts_file().lines().count(), 2, "loopback alone");
}

/// The cap counts distinct addresses: one past it refuses the lease, and a
/// list at it, repeats included, does not.
#[test]
fn test_an_allowlist_past_the_cap_is_refused() {
    let at_cap: Vec<_> = (0..ALLOWLIST_ADDRESSES_MAX)
        .map(|n| {
            let [_, _, high, low] = u32::try_from(n).unwrap().to_be_bytes();
            entry("host.example", [10, 1, high, low])
        })
        .collect();
    let mut repeated = at_cap.clone();
    repeated.push(entry("again.example", [10, 1, 0, 0]));
    let mut over = at_cap.clone();
    over.push(entry("one.more.example", [10, 2, 0, 0]));

    assert_eq!(
        Allowlist::new(at_cap).unwrap().addresses().len(),
        ALLOWLIST_ADDRESSES_MAX
    );
    assert_eq!(
        Allowlist::new(repeated).unwrap().addresses().len(),
        ALLOWLIST_ADDRESSES_MAX,
        "a repeat adds no address"
    );
    let refused = Allowlist::new(over).unwrap_err();
    assert_eq!(
        refused.egress_refusal(),
        Some(&EgressRefusal::TooManyAddresses(
            ALLOWLIST_ADDRESSES_MAX + 1
        ))
    );
}

/// Two resolutions that answered the same addresses, rotated and repeated as a
/// round-robin resolver answers, are one allowlist: names keep their merge
/// order, each name's addresses read sorted and once.
#[test]
fn test_a_rotated_answer_is_the_same_allowlist() {
    let first = Allowlist::new(vec![
        entry("a.example", [10, 0, 0, 2]),
        entry("a.example", [10, 0, 0, 1]),
        entry("b.example", [10, 0, 0, 9]),
    ])
    .unwrap();
    let rotated = Allowlist::new(vec![
        entry("a.example", [10, 0, 0, 1]),
        entry("b.example", [10, 0, 0, 9]),
        entry("a.example", [10, 0, 0, 2]),
        entry("a.example", [10, 0, 0, 1]),
    ])
    .unwrap();

    assert_eq!(first, rotated);
    assert_eq!(
        rotated.hosts_file(),
        "127.0.0.1 localhost\n::1 localhost\n\
         10.0.0.1 a.example\n10.0.0.2 a.example\n10.0.0.9 b.example\n"
    );
}
