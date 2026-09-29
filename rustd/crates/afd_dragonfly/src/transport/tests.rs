//! The connect ladder's arithmetic, the authority's reach, and how a failed
//! dial is named.
#![expect(
    clippy::expect_used,
    reason = "a unit test asserts by panicking; the crate's restriction set is for the daemon"
)]

use std::time::Duration;

use super::{
    CONNECT_ATTEMPT_TIMEOUT, CONNECT_ATTEMPTS, RETRY_MAX_WAIT, builder, judged, recovered,
};
use crate::config::{DragonflyConfig, DragonflyRole};

fn default_budget() -> Duration {
    DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:6379".to_owned())
        .connect_timeout()
}

/// The regression this pins, and the reason it is a correctness test rather
/// than a performance one: while the dial ladder fits the budget, the
/// driver's own error always arrives first and keeps its source chain. Any
/// change to the constants has to preserve it. Jitter is additive, so a
/// retry's ceiling is twice its capped wait.
#[test]
fn test_the_connect_ladder_answers_before_the_budget_expires() {
    let attempts_made = u32::try_from(CONNECT_ATTEMPTS.get()).unwrap_or(u32::MAX);
    let attempts = CONNECT_ATTEMPT_TIMEOUT * attempts_made;
    let sleeps = RETRY_MAX_WAIT * 2 * attempts_made.saturating_sub(1);
    let worst = attempts + sleeps;
    let budget = default_budget();
    assert!(
        worst < budget,
        "dials plus jittered backoff must finish inside the connect budget, \
         or the driver is cancelled mid-retry and its error is lost: \
         worst case {worst:?} against a {budget:?} budget",
    );
}

/// A configured authority does not make a plaintext seed a TLS one: the
/// scheme selects the transport, the authority only says whom to trust
/// once TLS is chosen. The path names a file that does not exist, and that
/// is the assertion — the plaintext branch never reads it.
#[test]
fn test_a_configured_authority_does_not_force_tls_on_a_plaintext_url() {
    let config = DragonflyConfig::from_url(DragonflyRole::Api, "redis://127.0.0.1:6379".to_owned())
        .with_ca_cert_file(Some("/nonexistent/authority.pem".into()));
    assert!(
        builder(&config, Duration::from_secs(1)).is_ok(),
        "a redis:// seed opens plaintext whatever authority is configured"
    );
}

/// And the scheme that does mean TLS still reaches the authority, failing
/// on the file rather than quietly opening plaintext to a TLS port.
#[test]
fn test_a_tls_url_reads_the_authority_it_was_given() {
    let config =
        DragonflyConfig::from_url(DragonflyRole::Api, "rediss://127.0.0.1:6380".to_owned())
            .with_ca_cert_file(Some("/nonexistent/authority.pem".into()));
    let refusal = builder(&config, Duration::from_secs(1))
        .err()
        .map(|error| error.to_string());
    assert!(
        refusal
            .as_deref()
            .is_some_and(|message| message.contains("/nonexistent/authority.pem")),
        "a rediss:// seed must consult the authority and name it when unreadable: {refusal:?}"
    );
}

fn driver_error(detail: &str) -> redis::RedisError {
    redis::RedisError::from((redis::ErrorKind::Io, "dial failed", detail.to_owned()))
}

fn tls_config() -> DragonflyConfig {
    DragonflyConfig::from_url(DragonflyRole::Api, "rediss://127.0.0.1:6380".to_owned())
}

/// A diagnosis dial offers a cause only when it failed: one that connected, or
/// ran out its own time, has nothing to prefer over the error already held.
#[tokio::test]
async fn a_diagnosis_offers_a_cause_only_when_it_failed() {
    let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
        .await
        .expect_err("a pending future never beats a zero deadline");
    assert!(
        recovered::<()>(Err(elapsed)).is_none(),
        "timed out: no cause"
    );
    assert!(recovered(Ok(Ok(()))).is_none(), "connected: no cause");
    let cause = recovered::<()>(Ok(Err(driver_error("UnknownIssuer"))));
    assert!(cause.is_some_and(|error| error.to_string().contains("UnknownIssuer")));
}

/// A TLS dial whose kept error hides the refusal is reported as the
/// certificate rejection the diagnosis recovered, carrying that cause.
#[test]
fn a_certificate_the_diagnosis_recovers_is_a_rejection() {
    let error = judged(
        &tls_config(),
        driver_error("attempt timed out"),
        Some(driver_error("invalid peer certificate: UnknownIssuer")),
    );
    assert!(error.is_certificate_rejected(), "{error}");
    let chain = format!("{error:?}");
    assert!(
        chain.contains("UnknownIssuer"),
        "the recovered cause is kept: {chain}"
    );
}

/// A diagnosis that names no certificate leaves the first failure standing,
/// and the first failure naming one needs no diagnosis at all.
#[test]
fn only_a_named_certificate_turns_a_failure_into_a_rejection() {
    let unreachable = judged(
        &tls_config(),
        driver_error("connection refused"),
        Some(driver_error("connection refused again")),
    );
    assert!(!unreachable.is_certificate_rejected() && unreachable.is_unavailable());
    assert!(format!("{unreachable:?}").contains("connection refused"));

    let first = judged(&tls_config(), driver_error("bad certificate"), None);
    assert!(first.is_certificate_rejected(), "{first}");
}
