//! Runner credential maintenance against live Postgres.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use crate::requests;
use crate::support;
use afd_auth::credential::{CredentialKind, Presented};
use afd_auth::directory::{CredentialDirectory as _, Digest};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_state::Credentials;
use afd_wire::admin::RunnerAdminAction;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;

const ACTOR: &str = "fixture:platform-operator";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_selftest_request_distinguishes_missing_and_revoked() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");

    let requested = fixtures
        .runners()
        .request_selftest(
            &enrolled.runner_id,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("a live runner records the ask");
    assert_eq!(requested.requested_at(), ENROLLED_AT + 1);
    let expected = (ENROLLED_AT + 1).to_string();
    assert_eq!(
        fixtures
            .runner_column(enrolled.runner_id.as_str(), "selftest_requested_at")
            .await
            .as_deref(),
        Some(expected.as_str())
    );

    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("the runner is revoked");
    let revoked = fixtures
        .runners()
        .request_selftest(
            &enrolled.runner_id,
            UnixMillis::from_millis(ENROLLED_AT + 3),
        )
        .await
        .expect_err("a revoked runner cannot collect an ask");
    assert_eq!(revoked.code().as_str(), "UZ-RUN-018");

    let missing = Uuid7::parse("019329c5-0000-7000-8000-0000000000ff")
        .expect("the missing fixture is a UUIDv7");
    let absent = fixtures
        .runners()
        .request_selftest(&missing, UnixMillis::from_millis(ENROLLED_AT + 4))
        .await
        .expect_err("a missing runner stays distinct from revocation");
    assert_eq!(absent.code().as_str(), "UZ-RUN-014");

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_rotation_takeover() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    let old_digest = Digest::of_minted(enrolled.token.expose());
    let directory = Credentials::new(fixtures.database.clone());
    assert!(
        directory
            .resolve(CredentialKind::RunnerToken, &old_digest)
            .await
            .expect("the directory answers")
            .is_some()
    );

    let rotated = fixtures
        .runners()
        .rotate_token(
            &enrolled.runner_id,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("a live runner credential rotates");
    let presented = Presented::from_authorization(&format!("Bearer {}", rotated.expose()))
        .expect("the minted token is presentable");
    let new_digest = Digest::of(&presented);

    assert!(
        directory
            .resolve(CredentialKind::RunnerToken, &old_digest)
            .await
            .expect("the old lookup answers")
            .is_none(),
        "the old credential stops authenticating immediately"
    );
    assert!(
        directory
            .resolve(CredentialKind::RunnerToken, &new_digest)
            .await
            .expect("the new lookup answers")
            .is_some(),
        "the replacement credential authenticates"
    );
    assert_eq!(
        fixtures.events(enrolled.runner_id.as_str()).await,
        vec![
            "runner_registered".to_owned(),
            "runner_token_rotated".to_owned()
        ]
    );

    fixtures.cleanup().await;
}
