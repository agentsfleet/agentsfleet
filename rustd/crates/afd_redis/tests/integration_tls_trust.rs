//! The lane's TLS, proven where proving it means something.
//!
//! The rest of the lane connects over plaintext, because a handshake that
//! re-proves an unchanging certificate authority two hundred times costs a
//! 232 ms median against 0.1 ms and builds the queue that turns a healthy
//! Redis into `ConnectTimeout`. TLS is not thereby unproven; it is proven
//! here, and proven HARDER than the all-TLS lane managed.
//!
//! # What an all-TLS lane could not tell you
//!
//! Every connect in it presented the correct certificate authority, so every
//! connect succeeded — and a lane that had silently stopped verifying anything
//! would have passed identically. Success against a good certificate is not
//! evidence of verification. Refusal of a bad one is, and that is the case no
//! suite held until this one.
//!
//! So the dimension here is the trust DECISION, in both directions: the lane's
//! own authority is accepted, and a well-formed authority that did not sign
//! this server is refused.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::config::{RedisConfig, RedisRole};

/// The TLS endpoint, which is NOT the one the rest of the lane uses.
const TLS_URL_KNOB: &str = "TEST_REDIS_TLS_URL";

/// The authority that signed the lane's Redis certificate.
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// A well-formed authority that signed nothing on this machine.
const FOREIGN_CA_KNOB: &str = "TEST_REDIS_FOREIGN_CA";

fn lane(knob: &str) -> String {
    std::env::var(knob)
        .unwrap_or_else(|_| panic!("{knob} unset — run through `make test-integration-rustd`"))
}

fn tls_config() -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, lane(TLS_URL_KNOB))
}

/// The accepting direction: the lane's authority verifies this server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_the_lanes_authority_verifies_the_lanes_redis() {
    let config = tls_config().with_ca_cert_file(Some(lane(CA_KNOB).into()));

    let redis = afd_redis::test_util::connect_live(&config)
        .await
        .expect("the lane's own certificate authority must verify its own Redis");

    redis
        .ping()
        .await
        .expect("a verified TLS connection must serve");
}

/// The refusing direction, which is the one that actually proves verification.
///
/// A DIFFERENT authority, well-formed and valid, that simply did not sign this
/// server — generated beside the real one by the compose recipe, so it carries
/// no expiry a committed fixture would eventually hit. If verification were
/// disabled — a `danger_accept_invalid_certs` creeping in, a trust store
/// quietly falling back to system roots — the accepting test above would still
/// pass and only this one would fail. That asymmetry is the whole point.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_a_foreign_authority_is_refused_by_the_lanes_redis() {
    let config = tls_config().with_ca_cert_file(Some(lane(FOREIGN_CA_KNOB).into()));

    let failure = afd_redis::test_util::connect_live(&config)
        .await
        .expect_err("an authority that did not sign this server must be refused");

    // The CLASS matters as much as the refusal. An endpoint that was simply
    // unreachable would also be an `Err` here and would prove nothing about
    // verification, so this asserts the class through the crate's public
    // surface and then requires the certificate to be named as the reason.
    assert!(
        failure.is_unavailable(),
        "a foreign authority must be refused as unavailable, not as {}",
        failure.code().as_str()
    );
    let rendered = format!("{failure:?}");
    assert!(
        rendered.contains("certificate") || rendered.contains("UnknownIssuer"),
        "the refusal must name the certificate as its reason, or this test \
         cannot tell verification from an unreachable port: {rendered}"
    );
}
