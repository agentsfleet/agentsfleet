//! Real ownership changes fence both optimistic acquisition and locked grants.

mod races;

use af_core::{clock, error_code, id::Uuid7};
use afd_auth::directory::Digest;
use afd_runner::capacity::{
    self,
    authority::{Authority, Decision, Refusal},
};
use sqlx::Connection as _;

use super::{Fixture, enrolment};

type TestResult<T = ()> = afd_runner::Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn enrol(fixture: &mut Fixture) -> TestResult<(Uuid7, Digest)> {
    let enrolled = fixture.runners.register(&enrolment(), clock::now()).await?;
    fixture.enrolled.push(enrolled.runner_id.clone());
    Ok((enrolled.runner_id, enrolled.token.digest().clone()))
}

async fn acquire(
    fixture: &Fixture,
    runner: &Uuid7,
    digest: &Digest,
    previous: Option<&Uuid7>,
) -> TestResult<Authority> {
    match fixture
        .runners
        .acquire_connection(runner, digest.clone(), previous, clock::now())
        .await?
    {
        Decision::Allowed(authority) => Ok(authority),
        Decision::Refused(_) => Err("expected the live opening to acquire ownership".into()),
    }
}

async fn can_claim(fixture: &Fixture, authority: &Authority) -> TestResult<bool> {
    let mut connection = fixture.database.acquire().await?;
    let mut transaction = connection.begin().await?;
    let available = capacity::lock(&mut transaction, authority)
        .await?
        .is_some_and(capacity::Capacity::can_claim);
    transaction.commit().await?;
    Ok(available)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replaced_connection_cannot_regain_ownership_through_automatic_resume() -> TestResult {
    let mut fixture = Fixture::create().await;
    let (runner, digest) = enrol(&mut fixture).await?;
    let first = acquire(&fixture, &runner, &digest, None).await?;
    assert!(can_claim(&fixture, &first).await?);
    let second = acquire(&fixture, &runner, &digest, None).await?;
    assert_ne!(first.connection(), second.connection());
    assert_eq!(
        fixture.runners.check_connection(&first).await?,
        Decision::Refused(Refusal::Superseded)
    );
    assert!(!can_claim(&fixture, &first).await?);
    assert!(can_claim(&fixture, &second).await?);
    let refused = fixture
        .runners
        .acquire_connection(
            &runner,
            digest.clone(),
            Some(first.connection()),
            clock::now(),
        )
        .await?;
    assert_eq!(refused, Decision::Refused(Refusal::Superseded));
    assert_eq!(
        fixture.runners.check_connection(&second).await?,
        Decision::Allowed(())
    );
    let resumed = acquire(&fixture, &runner, &digest, Some(second.connection())).await?;
    assert_eq!(
        fixture.runners.check_connection(&second).await?,
        Decision::Refused(Refusal::Superseded)
    );
    assert!(can_claim(&fixture, &resumed).await?);
    assert_eq!(
        Refusal::Superseded.code(),
        error_code::RUN_STREAM_SUPERSEDED
    );
    fixture.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rotation_refuses_old_acquisition_and_old_locked_grants() -> TestResult {
    let mut fixture = Fixture::create().await;
    let (runner, digest) = enrol(&mut fixture).await?;
    let old = acquire(&fixture, &runner, &digest, None).await?;
    let rotated = fixture
        .runners
        .rotate_token(&runner, "connection-test", clock::now())
        .await?;
    assert_eq!(
        fixture.runners.check_connection(&old).await?,
        Decision::Refused(Refusal::Credential)
    );
    assert!(!can_claim(&fixture, &old).await?);
    assert_eq!(
        fixture
            .runners
            .acquire_connection(&runner, digest, None, clock::now())
            .await?,
        Decision::Refused(Refusal::Credential)
    );
    let current = acquire(&fixture, &runner, rotated.digest(), None).await?;
    assert!(can_claim(&fixture, &current).await?);
    assert_eq!(
        Refusal::Credential.code(),
        error_code::RUN_INVALID_RUNNER_TOKEN
    );
    fixture.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_connection_proof_never_authorizes_another_host_and_debug_omits_its_digest() -> TestResult
{
    let mut fixture = Fixture::create().await;
    let (runner, digest) = enrol(&mut fixture).await?;
    let (other, _) = enrol(&mut fixture).await?;
    let authority = acquire(&fixture, &runner, &digest, None).await?;
    let mut connection = fixture.database.acquire().await?;
    let mut transaction = connection.begin().await?;
    let guard = capacity::Guard::from((&other, &authority));
    assert!(!format!("{guard:?}").contains(digest.as_str()));
    assert!(!format!("{authority:?}").contains(digest.as_str()));
    assert!(capacity::lock(&mut transaction, guard).await?.is_none());
    transaction.commit().await?;
    drop(connection);
    fixture.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stopped_and_removed_hosts_return_terminal_classified_decisions() -> TestResult {
    use afd_api_wire::admin::RunnerAdminAction;
    let mut fixture = Fixture::create().await;
    for action in [
        RunnerAdminAction::Cordon,
        RunnerAdminAction::Drain,
        RunnerAdminAction::Revoke,
    ] {
        let (runner, digest) = enrol(&mut fixture).await?;
        let authority = acquire(&fixture, &runner, &digest, None).await?;
        fixture
            .runners
            .transition(&runner, action, "connection-test", clock::now())
            .await?;
        assert_eq!(
            fixture.runners.check_connection(&authority).await?,
            Decision::Refused(Refusal::Revoked)
        );
        assert_eq!(
            fixture
                .runners
                .acquire_connection(&runner, digest, None, clock::now())
                .await?,
            Decision::Refused(Refusal::Revoked)
        );
        assert!(!can_claim(&fixture, &authority).await?);
        if action == RunnerAdminAction::Revoke {
            fixture.runners.delete_revoked(&runner).await?;
            assert_eq!(
                fixture.runners.check_connection(&authority).await?,
                Decision::Refused(Refusal::Credential)
            );
        }
    }
    let absent = afd_db::test_util::mint_id();
    let digest = Digest::of_minted("absent-connection-fixture");
    assert_eq!(
        fixture
            .runners
            .acquire_connection(&absent, digest, None, clock::now())
            .await?,
        Decision::Refused(Refusal::Credential)
    );
    assert_eq!(Refusal::Revoked.code(), error_code::RUN_ADMIN_STATE_BLOCKED);
    fixture.cleanup().await;
    Ok(())
}
