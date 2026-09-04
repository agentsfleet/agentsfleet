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

use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::{Dedicated, Redis};

/// Long enough that a real connection attempt would finish, short enough that a
/// URL which somehow DID open a socket fails the test rather than hanging it.
const REFUSAL_BUDGET: Duration = Duration::from_secs(5);

/// The park a dedicated connection is opened for here. Irrelevant to a refusal
/// — nothing is read — but the constructor asks, and a test that made one up
/// per call would be asserting nothing about the value.
const PARK: Duration = Duration::from_millis(100);

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
/// A SECOND construction path — `build_with_tls` rather than `open` — and the
/// one every deployment actually takes, because the Redis this talks to serves
/// a self-signed certificate.
///
/// # The URL has to be `rediss://` or this proves nothing
///
/// `build_client` takes the TLS branch only when a CA is configured AND the URL
/// is a TLS one — a plain URL with a CA beside it falls through to
/// `Client::open`, which is the branch the test above already covers. A version
/// of this test on a `redis://` URL passed for that reason while
/// `build_with_tls` never ran.
#[tokio::test(flavor = "multi_thread")]
async fn test_the_tls_client_carries_the_same_refusal() {
    let ca_path = std::env::temp_dir().join(format!(
        "afd_redis_connect_refusals_{}.pem",
        std::process::id()
    ));
    // Readable, so the certificate-unreadable branch is NOT what this reaches,
    // and not a certificate, so `build_with_tls` is what refuses it. The
    // distinction is the whole test: an unreadable file and an unparseable one
    // are different failures, and only the second reaches the client at all.
    std::fs::write(
        &ca_path,
        b"-----BEGIN CERTIFICATE-----\nnot a certificate\n",
    )
    .expect("the temp directory must be writable");

    let config = RedisConfig::from_url(RedisRole::Api, "rediss://127.0.0.1:1/".to_owned())
        .with_ca_cert_file(Some(ca_path.clone()));
    assert!(
        config.is_tls(),
        "a plain URL would take the non-TLS branch and prove nothing here"
    );

    let error = tokio::time::timeout(REFUSAL_BUDGET, Redis::connect(&config))
        .await
        .expect("must fail fast, not hang")
        .expect_err("an unparseable certificate authority must not produce a client");

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

/// A dedicated connection to a Redis that is not there names its role too.
///
/// Its own construction path: [`Dedicated::connect`] deliberately skips the
/// ping [`Redis::connect`] does, so a caller could reasonably expect it to
/// succeed and fail later. It does not — the multiplexed connection is opened
/// eagerly — and the failure has to carry the role for the same reason the
/// shared client's does: a deployment runs two, and an operator told only
/// "Redis is unreachable" edits the wrong connection string.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_dedicated_connection_to_nothing_is_refused_by_role() {
    for role in RedisRole::ALL {
        // A port nothing listens on, rather than a malformed URL: the client
        // BUILDS here and the connection is what fails, which is the branch a
        // deployment meets when Redis is down rather than misconfigured.
        let config = RedisConfig::from_url(*role, "redis://127.0.0.1:1/".to_owned());

        let error = tokio::time::timeout(REFUSAL_BUDGET, Dedicated::connect(&config, PARK))
            .await
            .expect("a refused connection must fail fast, not hang")
            .expect_err("nothing listens there, so no connection can open");

        assert!(
            error.is_unavailable(),
            "{role:?} gave the wrong class: {error}"
        );
        assert!(
            error.to_string().contains(role.tag()),
            "the failure must name the role: {error}"
        );
    }
}
