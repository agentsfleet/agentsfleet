//! What `Redis::connect` does when the client cannot even be built.
//!
//! These need no server, which is the point: the failures here happen before a
//! socket is opened, and a suite that only ever pointed at a live Redis would
//! never reach them. Both spellings of the client — plain and
//! certificate-authority-pinned — carry their own construction failure, and a
//! deployment that pasted a broken `REDIS_URL` meets one of them at boot.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

/// Long enough that a real connection attempt would finish, short enough that a
/// URL which somehow DID open a socket fails the test rather than hanging it.
const REFUSAL_BUDGET: Duration = Duration::from_secs(5);

/// A URL the client cannot be built from is an unreachable-Redis failure that
/// names the role, not a panic and not a hang.
///
/// The role matters: a deployment runs two of these, and "Redis is unreachable"
/// without saying which one sends an operator to the wrong connection string.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_url_the_client_cannot_be_built_from_is_refused_by_role() {
    for bad in [
        "not-a-url",
        "http://localhost:6379",
        "redis://[::1",
        "",
        "redis://user:pw@host:99999",
    ] {
        for role in RedisRole::ALL {
            let config = RedisConfig::from_url(*role, bad.to_owned());
            let error = tokio::time::timeout(REFUSAL_BUDGET, Redis::connect(&config))
                .await
                .expect("a URL that cannot be parsed must fail fast, not hang")
                .expect_err("{bad:?} must not produce a client");

            assert!(
                error.is_unavailable(),
                "{bad:?} for {role:?} gave the wrong class: {error}"
            );
            let rendered = error.to_string();
            assert!(
                rendered.contains(role.tag()),
                "the failure must name the role: {rendered}"
            );
        }
    }
}

/// The certificate-authority-pinned client carries the same refusal.
///
/// It is a SECOND construction path — `build_with_tls` rather than `open` — and
/// it is the one every deployment actually takes, because the Redis this talks
/// to serves a self-signed certificate. A failure that only the untested branch
/// reported would surface first in production.
#[tokio::test(flavor = "multi_thread")]
async fn test_the_tls_client_carries_the_same_refusal() {
    let ca_path = std::env::temp_dir().join(format!(
        "afd_redis_connect_refusals_{}.pem",
        std::process::id()
    ));
    // Readable, so the certificate-unreadable branch is NOT what this reaches;
    // the build failure below is the client's, which is the branch under test.
    std::fs::write(&ca_path, b"-----BEGIN CERTIFICATE-----\n")
        .expect("the temp directory must be writable");

    let config = RedisConfig::from_url(RedisRole::Api, "not-a-url".to_owned())
        .with_ca_cert_file(Some(ca_path.clone()));
    let error = tokio::time::timeout(REFUSAL_BUDGET, Redis::connect(&config))
        .await
        .expect("must fail fast, not hang")
        .expect_err("a broken URL must not produce a TLS client");

    assert!(
        error.is_unavailable(),
        "the TLS path must report the same class as the plain one: {error}"
    );
    assert!(
        error.to_string().contains(RedisRole::Api.tag()),
        "the failure must name the role: {error}"
    );

    let _ = std::fs::remove_file(&ca_path);
}
