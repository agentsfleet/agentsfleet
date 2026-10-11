#![expect(
    clippy::unwrap_used,
    reason = "test target: a host every system carries resolves"
)]

use std::net::{IpAddr, Ipv4Addr};

use reqwest::Url;

use super::{Resolve as _, SystemResolver, blocked_literal, unblocked};

/// A public address, and one in a private range.
const PUBLIC: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
const PRIVATE: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3));

/// A name is reached at every address it answers with, or not at all: one
/// blocked answer beside a public one refuses the whole name.
#[test]
fn should_refuse_a_name_whole_when_any_of_its_addresses_is_blocked() {
    assert_eq!(unblocked(vec![PUBLIC]).ok(), Some(vec![PUBLIC]));
    assert_eq!(unblocked(vec![PUBLIC, PRIVATE]).ok(), None);
    assert_eq!(unblocked(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]).ok(), None);
    assert_eq!(unblocked(Vec::new()).ok(), Some(Vec::new()));
}

/// A literal is judged by the blocked ranges, an IPv4-mapped IPv6 spelling
/// included; a name is left to the resolver, whatever it would resolve to.
#[test]
fn should_judge_an_address_literal_and_leave_a_name_to_the_resolver() {
    for blocked in [
        "https://127.0.0.1/",
        "https://10.1.2.3/",
        "https://[::1]/",
        "https://[::ffff:10.1.2.3]/",
    ] {
        assert!(blocked_literal(&Url::parse(blocked).unwrap()), "{blocked}");
    }
    for left in [
        "https://192.0.2.1/",
        "https://localhost/",
        "https://example.com/",
    ] {
        assert!(!blocked_literal(&Url::parse(left).unwrap()), "{left}");
    }
}

/// The host's own resolver answers a name every host carries, offline.
#[tokio::test]
async fn should_resolve_through_the_hosts_own_resolver() {
    let addresses = SystemResolver.resolve("localhost").await.unwrap();

    assert!(
        addresses.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)),
        "{addresses:?}"
    );
}
