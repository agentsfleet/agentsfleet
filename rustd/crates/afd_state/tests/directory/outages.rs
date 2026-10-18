//! Dimension 5.5 — a datastore that cannot answer is `Unavailable`, never `Ok(None)`.
//!
//! The assertion the credential directory exists for. `Ok(None)` becomes a 401
//! the holder is told to fix and `Unavailable` a 503 it is told to retry; the
//! runner client counts the first toward a self-termination ceiling and resets
//! that counter on the second, so collapsing them turns a Postgres blip into a
//! fleet-wide shutdown, one runner at a time.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.

use afd_auth::credential::CredentialKind;
use afd_auth::directory::CredentialDirectory;
use afd_state::Credentials;

use crate::support::{Fixtures, digest_of, identifier};

/// A statement the datastore refuses is `Unavailable`, never `Ok(None)`.
///
/// This is the dimension's real assertion. Both failures answer the caller
/// differently: `Ok(None)` becomes a 401 the holder is told to fix, and
/// `Unavailable` becomes a 503 the holder is told to retry. The runner client
/// counts the first toward a self-termination ceiling and resets that counter
/// on the second, so collapsing them turns a Postgres blip into a fleet-wide
/// shutdown, one runner at a time.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_refused_statement_is_unavailable_not_unknown() {
    let fixtures = Fixtures::create_disposable().await;
    let tenant = fixtures.tenant(&identifier(30)).await;
    let digest = digest_of("agt_t_fixture_outage");
    fixtures
        .api_key(&tenant, &digest, "auth0|minter", true)
        .await;

    // The row exists and the credential is good; only the store is broken.
    fixtures.drop_table("identity.api_keys").await;

    let answer = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::TenantApiKey, &digest)
        .await;

    assert!(
        answer.is_err(),
        "a statement Postgres refused must not be reported as a credential \
         nobody holds, got {answer:?}"
    );

    fixtures.cleanup().await;
}

/// A datastore that cannot be reached at all is `Unavailable` too.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_unreachable_datastore_is_unavailable() {
    let fixtures = Fixtures::create_disposable().await;
    let directory = Credentials::new(fixtures.database().clone());
    fixtures.destroy_database().await;

    let answer = directory
        .resolve(CredentialKind::RunnerToken, &digest_of("agt_r_gone"))
        .await;

    assert!(
        answer.is_err(),
        "no connection means no answer, which is not the same as an answer of \
         no, got {answer:?}"
    );

    fixtures.cleanup().await;
}

/// A blank provider subject is `Unavailable`, not a principal with no identity.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_blank_subject_is_unavailable() {
    // A database of its own: `uq_users_oidc_subject` admits one blank subject,
    // and a blank cannot be salted per run the way the other suites' subjects
    // are, so the shared lane would refuse the second run's user.
    let fixtures = Fixtures::create_disposable().await;
    let tenant = fixtures.tenant(&identifier(41)).await;
    let user = fixtures.user(&identifier(42), &tenant, "").await;
    let digest = digest_of("afc_fixture_blank_subject");
    fixtures
        .cli_credential(&user, &tenant, &digest, false)
        .await;

    let answer = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::CliCredential, &digest)
        .await;

    assert!(
        answer.is_err(),
        "a blank subject resolves to no capabilities at the provider, so every \
         gate would refuse it anyway — refusing here names the credential that \
         carried it, got {answer:?}"
    );

    fixtures.cleanup().await;
}
