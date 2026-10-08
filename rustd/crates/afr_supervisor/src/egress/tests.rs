//! The lease's egress: which hosts are merged, how each policy maps to a
//! network, and why a host that cannot be admitted refuses the lease.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use afd_core::error_code::INTERNAL_OPERATION_FAILED;
use afd_wire::policy::NetworkPolicy as FleetNetwork;
use afd_wire::runner::NetworkPolicy;
use afr_sandbox::{ALLOWLIST_ADDRESSES_MAX, Allowlist, Network};

use super::{Bound, DEFAULT_REGISTRY, Egress, Named, Resolve, SystemResolver};
use crate::test_support::{FakeResolver, assigned};

/// Two registry hosts, and a fleet host one of them shares.
const A: &str = "a.example";
const B: &str = "b.example";
const C: &str = "c.example";
/// A host the resolver answers with IPv6 alone.
const V6_ONLY: &str = "v6.example";
/// A host the resolver answers with more addresses than a lease may reach.
const CROWDED: &str = "crowded.example";
/// A host the resolver has never heard of.
const UNKNOWN: &str = "unknown.example";
/// The address every named host answers with, and a second one.
const FIRST: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);
const SECOND: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 2);
/// Hosts the resolver answers with an address no fleet may reach: the cloud
/// metadata service, a private range, the tailnet's shared range, loopback,
/// the metadata service in an IPv4-mapped IPv6 spelling, and a public address
/// beside a private one.
const METADATA: &str = "metadata.example";
const LAN: &str = "lan.example";
const TAILNET: &str = "tailnet.example";
const LOOPBACK: &str = "loopback.example";
const MAPPED: &str = "mapped.example";
const MIXED: &str = "mixed.example";
const METADATA_ADDRESS: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);
const LAN_ADDRESS: Ipv4Addr = Ipv4Addr::new(10, 1, 2, 3);
const TAILNET_ADDRESS: Ipv4Addr = Ipv4Addr::new(100, 101, 102, 103);

/// A fleet's network block allowing `hosts`, read-only or not.
fn fleet(hosts: &[&'static str], read_only: bool) -> FleetNetwork<'static> {
    FleetNetwork {
        allow: hosts.iter().copied().map(Cow::Borrowed).collect(),
        read_only,
        read_post_paths: Vec::new(),
    }
}

fn allow_list(registry: &[&'static str]) -> Egress {
    assigned(NetworkPolicy::AllowListEgress, registry)
}

/// More distinct addresses than an allowlist may hold.
fn crowd() -> Vec<IpAddr> {
    (0..=ALLOWLIST_ADDRESSES_MAX)
        .map(|index| {
            let [_, _, high, low] = u32::try_from(index).unwrap().to_be_bytes();
            IpAddr::V4(Ipv4Addr::new(10, 0, high, low))
        })
        .collect()
}

fn resolver() -> FakeResolver {
    let v4 = [IpAddr::V4(FIRST)];
    FakeResolver::answering(&[
        (A, &v4),
        (B, &[IpAddr::V4(SECOND), IpAddr::V6(Ipv6Addr::LOCALHOST)]),
        (C, &v4),
        (V6_ONLY, &[IpAddr::V6(Ipv6Addr::LOCALHOST)]),
        (CROWDED, &crowd()),
    ])
}

/// The answers each blocked host resolves to, and one public host.
fn blocking_resolver() -> FakeResolver {
    FakeResolver::answering(&[
        (METADATA, &[IpAddr::V4(METADATA_ADDRESS)]),
        (LAN, &[IpAddr::V4(LAN_ADDRESS)]),
        (TAILNET, &[IpAddr::V4(TAILNET_ADDRESS)]),
        (LOOPBACK, &[IpAddr::V4(Ipv4Addr::LOCALHOST)]),
        (MAPPED, &[IpAddr::V6(METADATA_ADDRESS.to_ipv6_mapped())]),
        (MIXED, &[IpAddr::V4(FIRST), IpAddr::V4(LAN_ADDRESS)]),
        (A, &[IpAddr::V4(FIRST)]),
    ])
}

/// Each host's name, dropping who named it.
fn names(hosts: &[(String, Named)]) -> Vec<&str> {
    hosts.iter().map(|(host, _named)| host.as_str()).collect()
}

/// Registry hosts come first, then the fleet's, each once in the order first
/// named; a read-only fleet adds none, and a registry left empty is the
/// runner's default set.
#[test]
fn test_egress_plan_merges_and_excludes_read_only() {
    let egress = allow_list(&[A, B]);

    let (registry, from_fleet) = (Named::Registry, Named::Fleet);
    assert_eq!(
        egress.hosts(&fleet(&[B, C], false)),
        [(A, registry), (B, registry), (C, from_fleet)]
            .map(|(host, named)| (host.to_owned(), named))
    );
    assert_eq!(names(&egress.hosts(&fleet(&[B, C], true))), [A, B]);
    assert_eq!(
        names(&allow_list(&[]).hosts(&fleet(&[], false))),
        DEFAULT_REGISTRY,
        "an operator who named no registry gets the runner's own"
    );
}

/// An entry may carry a port, a scheme or a path, and either case; the
/// kernel set admits addresses, so only the host is kept, and two spellings
/// of one host are one host. An entry no host can be read from stays as
/// written, for the resolver to refuse by name.
#[test]
fn an_entry_keeps_its_host_and_drops_its_port_scheme_and_path() {
    let egress = allow_list(&["a.example:443", "A.Example:8443", B]);
    let authored = fleet(
        &["https://c.example/v1", "c.example:443", "[::1]:80", "a b"],
        false,
    );

    assert_eq!(names(&egress.hosts(&authored)), [A, B, C, "::1", "a b"]);
}

/// Each posture maps to its network: the host's, none, or the resolved
/// allowlist, whose addresses are only the IPv4 ones.
#[tokio::test]
async fn each_policy_binds_to_its_network() {
    let resolver = resolver();
    let fleet = fleet(&[C], false);
    let bind = |egress: Egress| {
        let (fleet, resolver) = (&fleet, &resolver);
        async move { egress.bind(fleet, resolver).await.unwrap() }
    };

    let open = bind(assigned(NetworkPolicy::AllowAll, &[A])).await;
    let shut = bind(assigned(NetworkPolicy::DenyAllEgress, &[A])).await;
    let listed = bind(allow_list(&[A, B])).await;

    assert_eq!(open, Bound::Host);
    assert_eq!(shut, Bound::Isolated);
    let expected = Allowlist::new(vec![
        (A.to_owned(), FIRST),
        (B.to_owned(), SECOND),
        (C.to_owned(), FIRST),
    ])
    .unwrap();
    assert_eq!(listed, Bound::Allowed(expected));
    assert_eq!(open.network(), Network::Host);
    assert_eq!(shut.network(), Network::Isolated);
    assert!(matches!(listed.network(), Network::Allowed(_)));
}

/// A runner with no readable assignment reaches nothing.
#[tokio::test]
async fn a_closed_egress_reaches_nothing() {
    let bound = Egress::closed()
        .bind(&fleet(&[A], false), &resolver())
        .await
        .unwrap();

    assert_eq!(bound, Bound::Isolated);
}

/// An unknown host, a host with no IPv4 address and an allowlist past its cap
/// each refuse with a reason of their own, the first two naming the host and
/// the third carrying the engine's reason; each is logged under the internal
/// code a runner failure is read by.
#[tokio::test]
async fn test_egress_setup_failures_refuse_the_lease() {
    let resolver = resolver();
    let refused = |host: &'static str| {
        let resolver = &resolver;
        async move {
            allow_list(&[host])
                .bind(&fleet(&[], false), resolver)
                .await
                .unwrap_err()
        }
    };

    let unknown = refused(UNKNOWN).await;
    let v6_only = refused(V6_ONLY).await;
    let crowded = refused(CROWDED).await;

    let expected = [
        (
            &unknown,
            format!("egress host {UNKNOWN} could not be resolved"),
        ),
        (
            &v6_only,
            format!("egress host {V6_ONLY} resolves to no IPv4 address"),
        ),
        (
            &crowded,
            "the lease's egress allowlist was refused".to_owned(),
        ),
    ];
    for (failure, reason) in expected {
        assert!(failure.to_string().contains(&reason), "{failure}");
        assert_eq!(failure.code(), INTERNAL_OPERATION_FAILED, "{failure}");
        assert!(!failure.is_egress_blocked(), "{failure}");
    }
    let crowded_cause = std::error::Error::source(&crowded)
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(crowded_cause.contains("past the"), "{crowded_cause}");
}

/// A host the fleet names is refused when any address it resolves to is one
/// no fleet may reach, IPv6 spellings and a blocked answer beside a public one
/// included. The refusal names the host, never the address.
#[tokio::test]
async fn test_a_fleet_host_at_a_blocked_address_refuses_the_lease() {
    let resolver = blocking_resolver();
    let cases = [
        (METADATA, IpAddr::V4(METADATA_ADDRESS)),
        (LAN, IpAddr::V4(LAN_ADDRESS)),
        (TAILNET, IpAddr::V4(TAILNET_ADDRESS)),
        (LOOPBACK, IpAddr::V4(Ipv4Addr::LOCALHOST)),
        (MAPPED, IpAddr::V6(METADATA_ADDRESS.to_ipv6_mapped())),
        (MIXED, IpAddr::V4(LAN_ADDRESS)),
    ];

    for (host, address) in cases {
        let failure = allow_list(&[A])
            .bind(&fleet(&[host], false), &resolver)
            .await
            .unwrap_err();

        assert!(failure.is_egress_blocked(), "{host}: {failure}");
        let reason = failure.to_string();
        assert!(reason.contains(host), "{reason}");
        assert!(
            !reason.contains(&address.to_string()),
            "no address is named: {reason}"
        );
        assert_eq!(failure.code(), INTERNAL_OPERATION_FAILED, "{failure}");
    }
}

/// An address literal in `network.allow`, bracketed IPv6 and an IPv4-mapped
/// spelling included, is judged as the address it is: the host's resolver
/// parses it offline, and the lease is refused as blocked rather than as a
/// host that would not resolve.
#[tokio::test]
async fn test_an_address_literal_in_network_allow_is_judged_as_an_address() {
    for entry in [
        "[::1]:80",
        "http://[::ffff:169.254.169.254]/",
        "169.254.169.254",
    ] {
        let failure = allow_list(&["localhost"])
            .bind(&fleet(&[entry], false), &SystemResolver)
            .await
            .unwrap_err();

        assert!(failure.is_egress_blocked(), "{entry}: {failure}");
    }
}

/// The operator's registry may point at its own network, a package mirror on
/// a private range say, and a host both the registry and the fleet name is
/// the registry's.
#[tokio::test]
async fn test_a_registry_host_at_a_private_address_is_admitted() {
    let bound = allow_list(&[LAN])
        .bind(&fleet(&[LAN, A], false), &blocking_resolver())
        .await
        .unwrap();

    let expected =
        Allowlist::new(vec![(LAN.to_owned(), LAN_ADDRESS), (A.to_owned(), FIRST)]).unwrap();
    assert_eq!(bound, Bound::Allowed(expected));
}

/// The host's own resolver answers a name every host carries, offline.
#[tokio::test]
async fn the_system_resolver_answers_from_the_host() {
    let addresses = SystemResolver.resolve("localhost").await.unwrap();

    assert!(
        addresses.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)),
        "{addresses:?}"
    );
}
