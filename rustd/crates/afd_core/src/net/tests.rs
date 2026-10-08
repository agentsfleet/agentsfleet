#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::net::IpAddr;

use super::is_blocked;

fn blocks(address: &str) -> bool {
    is_blocked(address.parse::<IpAddr>().expect("an address literal"))
}

/// Every reserved IPv4 range — loopback, private, link-local, this-network,
/// multicast, reserved and broadcast — at its edges.
#[test]
fn test_v4_blocklist_covers_every_reserved_range() {
    for blocked in [
        "127.0.0.1",
        "127.255.255.255",
        "10.0.0.0",
        "10.255.255.255",
        "172.16.5.9",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "0.0.0.0",
        "0.1.2.3",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert!(blocks(blocked), "{blocked} must be blocked");
    }
}

/// Shared address space: the tailnet the dev runners sit on numbers its peers
/// from it, and Alibaba Cloud's metadata service lives inside it.
#[test]
fn test_v4_shared_address_space_is_blocked_to_its_edges() {
    for blocked in [
        "100.64.0.0",
        "100.100.100.200",
        "100.127.255.255",
        "::ffff:100.64.0.1",
    ] {
        assert!(blocks(blocked), "{blocked} must be blocked");
    }
    for allowed in ["100.63.255.255", "100.128.0.0"] {
        assert!(!blocks(allowed), "{allowed} must be allowed");
    }
}

#[test]
fn test_v4_public_boundaries_are_not_over_blocked() {
    // A /12 or /16 widened by one octet is how an SSRF guard quietly stops a
    // tenant from reaching their own gateway.
    for allowed in [
        "172.15.0.1",
        "172.32.0.1",
        "169.253.0.1",
        "169.255.0.1",
        "223.255.255.255",
        "8.8.8.8",
        "1.1.1.1",
    ] {
        assert!(!blocks(allowed), "{allowed} must be allowed");
    }
}

#[test]
fn test_v6_blocklist_covers_every_range_and_the_mapped_forms() {
    for blocked in [
        "::1",
        "::",
        "fc00::1",
        "fd12::3",
        "fe80::1",
        "febf::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
    ] {
        assert!(blocks(blocked), "{blocked} must be blocked");
    }
    for allowed in ["2606:4700:4700::1111", "fec0::1", "::ffff:8.8.8.8"] {
        assert!(!blocks(allowed), "{allowed} must be allowed");
    }
}

#[test]
fn should_refuse_every_v6_spelling_of_a_private_v4_address() {
    for spelled in [
        "64:ff9b::a00:5",
        "64:ff9b::a9fe:a9fe",
        "64:ff9b:1::a00:5",
        "2002:a00:5::1",
        "2002:a9fe:a9fe::",
        "::a00:5",
        "::7f00:1",
    ] {
        assert!(blocks(spelled), "{spelled} reaches a private address");
    }
}

#[test]
fn should_pass_a_v6_spelling_of_a_public_v4_address() {
    for spelled in ["64:ff9b::808:808", "2002:808:808::1", "2606:4700::1111"] {
        assert!(!blocks(spelled), "{spelled} is public");
    }
}
