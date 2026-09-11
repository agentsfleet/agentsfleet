//! Pre-flight tests that bind profiles to datastore targets.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::net::SocketAddr;

use super::{Profile, Target, binding_covers, endpoint_socket, published_sockets};

#[test]
fn test_compose_bindings_cover_only_their_address_family_and_port() {
    let bindings: Vec<SocketAddr> = published_sockets(b"0.0.0.0:20735\n[::]:20736\n").collect();
    let ipv4: SocketAddr = "127.0.0.1:20735".parse().expect("valid socket");
    let wrong_family: SocketAddr = "[::1]:20735".parse().expect("valid socket");
    let wrong_port: SocketAddr = "127.0.0.1:20736".parse().expect("valid socket");

    assert!(
        bindings
            .iter()
            .any(|binding| binding_covers(*binding, ipv4))
    );
    assert!(
        !bindings
            .iter()
            .any(|binding| binding_covers(*binding, wrong_family))
    );
    assert!(
        !bindings
            .iter()
            .any(|binding| binding_covers(*binding, wrong_port))
    );
    assert!(
        published_sockets(b"not a published port\n")
            .next()
            .is_none()
    );
}

#[test]
fn test_rig_endpoints_require_literal_loopback_addresses() {
    assert_eq!(
        endpoint_socket(
            "database endpoint",
            "postgres://user:secret@127.0.0.1:20735/database"
        )
        .expect("literal loopback is deterministic"),
        "127.0.0.1:20735".parse().expect("valid socket")
    );
    endpoint_socket("database endpoint", "postgres://localhost:20735/database")
        .expect_err("a hostname does not bind one published address family");
}

#[test]
fn test_shared_deployment_refuses_saturation_profile() {
    let target = Target::Rig;
    let remote_database = "postgres://bench:secret@db.example.invalid/agentsfleet";
    let remote_redis = "rediss://:secret@cache.example.invalid:6379";

    for (database, redis, named_host) in [
        (
            remote_database,
            "redis://127.0.0.1:6379",
            "db.example.invalid",
        ),
        (
            "postgres://bench:secret@127.0.0.1/agentsfleet",
            remote_redis,
            "cache.example.invalid",
        ),
    ] {
        let refused = Profile::Rig
            .check_endpoints(&target, database, redis)
            .expect_err("a rig label must not authorize a remote datastore");
        let message = refused.to_string();
        assert!(
            message.contains(named_host),
            "names the refused host: {message}"
        );
        assert!(
            !message.contains("secret"),
            "credentials never enter a refusal: {message}"
        );
        assert!(refused.is_pre_flight(), "no connection has opened yet");
    }

    Profile::Rig
        .check_endpoints(
            &target,
            "postgres://bench:secret@127.0.0.1:5432/agentsfleet",
            "redis://:secret@[::1]:6379",
        )
        .expect("both loopback forms belong to the owned rig");
}

#[test]
fn test_local_target_rejects_a_remote_discovered_node() {
    let refused = Target::Rig
        .check_discovered_host("advertised datastore node", "10.20.30.40")
        .expect_err("a safe seed cannot hide a remote advertised node");

    assert!(refused.to_string().contains("10.20.30.40"));
    Target::Rig
        .check_discovered_host("advertised datastore node", "127.0.0.1")
        .expect("the rig advertises loopback");
    assert!(
        Target::Rig
            .check_discovered_host("advertised datastore node", "local")
            .is_err(),
        "an arbitrary hostname named local is not loopback proof"
    );
}

#[test]
fn test_a_remote_profile_stays_disabled_until_it_has_a_verifiable_identity() {
    let target = Target::Deployed {
        address: "dev.example.invalid".to_owned(),
    };
    let refusal = Profile::Dev
        .check_endpoints(
            &target,
            "postgres://secret@db.example.invalid/agentsfleet",
            "rediss://secret@redis.example.invalid",
        )
        .expect_err("an address label does not prove ownership of two datastores");

    assert!(
        refusal
            .to_string()
            .contains("remote targets remain disabled")
    );
    assert!(!refusal.to_string().contains("secret"));
}
