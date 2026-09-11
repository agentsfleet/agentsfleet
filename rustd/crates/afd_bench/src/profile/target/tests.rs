//! Pre-flight tests that bind profiles to datastore targets.

use super::{Profile, Target};

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
