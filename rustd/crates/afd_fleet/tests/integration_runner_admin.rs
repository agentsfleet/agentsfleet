//! Operator mutations against live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_core::clock::UnixMillis;
use afd_auth::credential::{CredentialKind, Presented};
use afd_auth::directory::{CredentialDirectory as _, Digest};
use afd_state::Credentials;
use afd_wire::admin::RunnerAdminAction;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_admin_transitions() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    let runner = enrolled.runner_id.as_str().to_owned();

    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Cordon,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("an active runner may be cordoned");
    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Cordon,
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("repeating a transition is idempotent");
    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            UnixMillis::from_millis(ENROLLED_AT + 3),
        )
        .await
        .expect("a cordoned runner may be revoked");

    let refused = fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Drain,
            UnixMillis::from_millis(ENROLLED_AT + 4),
        )
        .await
        .expect_err("revocation is terminal");
    assert!(refused.is_rejected());
    assert_eq!(
        fixtures.runner_column(&runner, "admin_state").await,
        Some("revoked".to_owned())
    );
    assert_eq!(
        fixtures.events(&runner).await,
        vec![
            "runner_registered".to_owned(),
            "runner_cordoned".to_owned(),
            "runner_revoked".to_owned(),
        ],
        "each real transition appends one event; repeats and refusals append none"
    );

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
            "runner_token_rotated".to_owned(),
        ]
    );

    fixtures.cleanup().await;
}
