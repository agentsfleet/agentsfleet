//! Which hosts the guard refuses: every private range to its edges and in
//! its IPv6 spellings, a literal before any lookup, and a name whole when
//! any answer is private.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use reqwest::Url;
use reqwest::dns::Resolve as _;

use super::{Guarded, SystemLookup, is_denied, literal_admitted, reachable};
use crate::tools::code;
use crate::tools::outbound::Refused;
use crate::tools::outbound::testing::FixedLookup;

fn denies(address: &str) -> bool {
    is_denied(address.parse::<IpAddr>().expect("an address literal"))
}

fn at(address: &str) -> SocketAddr {
    SocketAddr::new(address.parse().expect("an address literal"), 443)
}

fn url(text: &str) -> Url {
    Url::parse(text).expect("a URL")
}

#[test]
fn the_guard_denies_every_private_v4_range_to_its_edges() {
    for denied in [
        "127.0.0.1",
        "127.255.255.255",
        "10.0.0.0",
        "10.255.255.255",
        "172.16.5.9",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.0",
        "100.100.100.200",
        "100.127.255.255",
        "192.0.0.0",
        "192.0.0.255",
        "0.0.0.0",
        "0.1.2.3",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert!(denies(denied), "{denied} must be denied");
    }
}

/// A range widened by one is how a guard quietly stops a request to a real
/// public host beside it.
#[test]
fn the_guard_admits_the_public_addresses_beside_each_range() {
    for admitted in [
        "9.255.255.255",
        "11.0.0.0",
        "172.15.0.1",
        "172.32.0.1",
        "169.253.0.1",
        "169.255.0.1",
        "100.63.255.255",
        "100.128.0.0",
        "191.255.255.255",
        "192.0.1.0",
        "192.0.2.1",
        "223.255.255.255",
        "8.8.8.8",
    ] {
        assert!(!denies(admitted), "{admitted} must be admitted");
    }
}

#[test]
fn the_guard_denies_every_private_v6_range_and_every_v6_spelling_of_a_private_v4() {
    for denied in [
        "::1",
        "::",
        "fc00::1",
        "fd12::3",
        "fe80::1",
        "febf::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "::ffff:100.64.0.1",
        "::7f00:1",
        "64:ff9b::a00:5",
        "64:ff9b:1::808:808",
        "2002:a00:5::1",
    ] {
        assert!(denies(denied), "{denied} must be denied");
    }
}

#[test]
fn the_guard_admits_public_v6_addresses_and_spellings() {
    for admitted in [
        "2606:4700:4700::1111",
        "fbff::1",
        "fec0::1",
        "::ffff:8.8.8.8",
        "64:ff9b::808:808",
        "2002:808:808::1",
    ] {
        assert!(!denies(admitted), "{admitted} must be admitted");
    }
}

#[test]
fn a_private_literal_is_refused_where_it_is_admitted() {
    for (literal, host) in [
        ("https://127.0.0.1/", "127.0.0.1"),
        ("https://10.0.0.1/x", "10.0.0.1"),
        ("https://[::1]/", "[::1]"),
        ("https://[fd00::1]:8443/", "[fd00::1]"),
        ("https://[::ffff:10.1.2.3]/", "[::ffff:a01:203]"),
    ] {
        let refused = literal_admitted(&url(literal)).map_err(Refused::from);
        assert_eq!(
            refused,
            Err(Refused::new(
                code::EGRESS_DENIED,
                format!("{host} is a private, loopback or reserved address"),
            )),
            "{literal}"
        );
    }
}

#[test]
fn a_public_literal_or_a_name_passes_to_the_lookup() {
    for passed in [
        "https://192.0.2.1/",
        "https://[2606:4700:4700::1111]/",
        "https://localhost/",
        "https://example.com/",
    ] {
        assert_eq!(literal_admitted(&url(passed)), Ok(()), "{passed}");
    }
}

#[tokio::test]
async fn a_name_with_one_private_answer_beside_a_public_one_is_refused_whole() {
    let lookup = FixedLookup::default()
        .answering("mixed.test", &[at("192.0.2.1"), at("10.0.0.1")])
        .answering("public.test", &[at("192.0.2.1")]);
    let guarded = Guarded::new(Arc::new(lookup));

    let mixed = guarded
        .resolve("mixed.test".parse().expect("a name"))
        .await
        .map(Iterator::collect::<Vec<_>>)
        .map_err(|failure| failure.to_string());
    let public = guarded
        .resolve("public.test".parse().expect("a name"))
        .await
        .map(Iterator::collect::<Vec<_>>)
        .map_err(|failure| failure.to_string());

    assert_eq!(
        mixed,
        Err("mixed.test resolves to a private, loopback or reserved address".to_owned())
    );
    assert_eq!(public, Ok(vec![at("192.0.2.1")]));
}

#[tokio::test]
async fn reachable_judges_a_name_by_what_the_lookup_answers() {
    let lookup = FixedLookup::default()
        .answering("inside.test", &[at("10.0.0.1")])
        .answering("outside.test", &[at("192.0.2.1")]);

    let inside = reachable(&url("https://inside.test/mcp"), &lookup).await;
    let outside = reachable(&url("https://outside.test/mcp"), &lookup).await;
    let unknown = reachable(&url("https://unknown.test/mcp"), &lookup).await;

    assert_eq!(
        inside,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "inside.test resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(outside, Ok(()));
    assert_eq!(
        unknown,
        Err(Refused::new(
            code::UPSTREAM_UNREACHABLE,
            "unknown.test could not be looked up: no answer for unknown.test",
        ))
    );
    assert_eq!(
        lookup.asked(),
        ["inside.test", "outside.test", "unknown.test"]
    );
}

#[tokio::test]
async fn reachable_refuses_a_private_literal_without_a_lookup() {
    let lookup = FixedLookup::default();

    let refused = reachable(&url("https://169.254.169.254/latest"), &lookup).await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "169.254.169.254 is a private, loopback or reserved address",
        ))
    );
    assert_eq!(lookup.asked(), Vec::<String>::new());
}

/// The host's own resolver answers the one name every host carries, offline.
#[tokio::test]
async fn the_system_lookup_answers_a_name_this_host_carries() {
    let answered: Vec<IpAddr> = SystemLookup
        .resolve("localhost".parse().expect("a name"))
        .await
        .map(|addresses| addresses.map(|address| address.ip()).collect())
        .map_err(|failure| failure.to_string())
        .expect("localhost resolves");

    assert!(
        answered.iter().all(IpAddr::is_loopback) && !answered.is_empty(),
        "{answered:?}"
    );
}
