//! The connection's own surface: what it reports about itself, and how each
//! way of failing to open or answer is told apart.
//!
//! Split from `integration_ready.rs` per RULE FLL: that file is the readiness
//! index, and this is the client every module rides on.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use crate::support::DragonflyHarness;

/// The connection answers for itself, and a certificate path that is not there
/// is a config failure rather than an outage.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_client_reports_its_own_configuration() {
    let harness = DragonflyHarness::connect().await;
    assert_eq!(harness.redis.role(), afd_dragonfly::DragonflyRole::Default);
    assert_eq!(
        harness.redis.request_timeout(),
        std::time::Duration::from_secs(5)
    );
    harness
        .redis
        .ping()
        .await
        .expect("a live Dragonfly answers PING");

    // TLS, because a trust anchor is what is being graded. On the lane's
    // plaintext endpoint a certificate authority is correctly ignored, so this
    // would connect happily and assert nothing.
    let missing_ca =
        DragonflyHarness::tls_config().with_ca_cert_file(Some("/nonexistent/ca.crt".into()));
    let error = afd_dragonfly::Dragonfly::connect(&missing_ca)
        .await
        .expect_err("a certificate authority that is not there must refuse");
    assert!(
        error.is_config(),
        "an unreadable certificate is a misconfiguration, not an outage: {error}"
    );
    assert!(!error.is_unavailable(), "got {error}");
}

/// A command that outlives its deadline is a timeout, named, not a hang.
///
/// Invariant 4 of the milestone is that every I/O deadline is a
/// `tokio::time::timeout` at the call site. A deadline nothing ever trips is
/// indistinguishable from no deadline at all, so this trips one deterministically:
/// `BLPOP` on a key nothing ever pushes to blocks the server for seconds, and
/// the client's budget is a fraction of that. Racing a fast command against a
/// tiny budget would not do — it completes inside the timer's first tick, which
/// is what the first version of this test discovered.
///
/// It also shows why a multiplexed connection must never carry a blocking
/// command in production: this one holds the socket for its whole duration,
/// which is why [`crate::streams`] reads never pass `BLOCK` and pub/sub gets a
/// connection of its own.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_a_command_past_its_deadline_is_a_timeout() {
    let harness = DragonflyHarness::connect().await;
    let impatient_config =
        DragonflyHarness::config().with_request_timeout(Duration::from_millis(50));
    let impatient = afd_dragonfly::Dragonfly::connect(&impatient_config)
        .await
        .expect("connecting is not the part under test");

    let key = harness.name("never_pushed");
    let mut blocking = redis::cmd("BLPOP");
    blocking.arg(&key).arg(5);

    let started = std::time::Instant::now();
    let error = impatient
        .command::<Option<Vec<String>>>("BLPOP", &key, &blocking)
        .await
        .expect_err("a 50ms budget cannot outlast a 5s block");

    assert!(
        error.is_unavailable(),
        "a deadline that passed is the datastore not answering in time: {error}"
    );
    assert!(!error.is_command(), "nothing was refused; nothing answered");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-004");
    assert!(
        error.to_string().contains("BLPOP"),
        "the failure must name the command that hung: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the deadline must cut the wait short, not ride it out: {:?}",
        started.elapsed()
    );
}

/// Every way opening a connection can fail, told apart.
///
/// Three different causes that a caller might otherwise see as one "could not
/// connect": a URL this client accepts but the driver does not, a certificate
/// file that exists and is not a certificate, and a port with nothing behind
/// it. The first two are misconfiguration an operator fixes in seconds once
/// the message says which; the third is an outage.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_connection_failures_name_their_cause() {
    use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

    // Our scheme check passes; the driver's own parser refuses the rest.
    let malformed =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://%%%invalid%%%".to_owned());
    let error = afd_dragonfly::Dragonfly::connect(&malformed)
        .await
        .expect_err("a URL the driver cannot parse must refuse");
    assert!(!error.is_command(), "nothing was ever sent: {error}");

    // A certificate authority file that is not a certificate.
    let junk = std::env::temp_dir().join(format!("afd-not-a-cert-{}.pem", std::process::id()));
    std::fs::write(&junk, b"this is not a certificate\n").expect("write the junk file");
    // TLS, for the reason the missing-authority case above records.
    let bad_pem = DragonflyHarness::tls_config().with_ca_cert_file(Some(junk.clone()));
    let error = afd_dragonfly::Dragonfly::connect(&bad_pem)
        .await
        .expect_err("a file that is not a certificate must refuse");
    assert!(!error.is_command(), "got {error}");
    let _ = std::fs::remove_file(&junk);

    // Nothing listening.
    let dead = DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned())
        .with_request_timeout(Duration::from_millis(500));
    let error = afd_dragonfly::Dragonfly::connect(&dead)
        .await
        .expect_err("nothing is listening on port 1");
    assert!(
        error.is_unavailable(),
        "an unreachable Dragonfly is an outage: {error}"
    );
}
