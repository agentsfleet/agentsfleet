//! Dimension 5.5 — the three stored classes resolve, and two answers stay apart.
//!
//! The assertion this suite exists for is the one about `Ok(None)` versus
//! `Err(Unavailable)`. Every other property here would still hold if those two
//! collapsed into each other, and collapsing them reports a Postgres outage as
//! an authentication rejection — which the runner client counts toward a
//! self-termination ceiling. A datastore blip would walk a healthy fleet's
//! runners to shutdown, one reject at a time.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/directory_fixtures.rs"]
mod support;

use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Liveness};
use afd_state::Credentials;

use self::support::{Fixtures, digest_of, identifier, identifier_with_bad_variant};

/// The `admin_state` a runner must hold to use the runner plane.
const ACTIVE: &str = "active";

/// A tenant api-key resolves to the person who minted it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_tenant_api_key_resolves_to_its_minter() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(1)).await;
    let digest = digest_of("agt_t_fixture_live");
    fixtures
        .api_key(&tenant, &digest, "auth0|minter", true)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::TenantApiKey, &digest)
        .await
        .expect("a reachable datastore answers");

    match found.expect("the key was seeded and must resolve") {
        CredentialRecord::Person {
            tenant: resolved,
            subject,
            live,
        } => {
            assert_eq!(resolved.as_str(), tenant);
            assert_eq!(
                subject.as_str(),
                "auth0|minter",
                "created_by holds the provider subject directly, which is why \
                 this class joins nothing"
            );
            assert_eq!(live, Liveness::Live);
        }
        record @ CredentialRecord::Machine { .. } => {
            panic!("a tenant api-key is a person's credential, got {record:?}")
        }
    }

    fixtures.cleanup().await;
}

/// A revoked key comes back as a row, not as nothing.
///
/// Filtering it out in SQL would make it indistinguishable from a key that
/// never existed, and the difference is what lets it answer `UZ-APIKEY-004` —
/// "this is dead, stop" — instead of a bare 401 the holder retries forever.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_revoked_key_resolves_and_says_it_is_revoked() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(2)).await;
    let digest = digest_of("agt_t_fixture_revoked");
    fixtures
        .api_key(&tenant, &digest, "auth0|minter", false)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::TenantApiKey, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person {
                live: Liveness::Revoked,
                ..
            })
        ),
        "a revoked key must resolve as a revoked row, got {found:?}"
    );

    fixtures.cleanup().await;
}

/// A command-line credential joins through to the person's provider subject.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_cli_credential_resolves_through_its_user() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(3)).await;
    let user = fixtures.user(&identifier(4), &tenant, "auth0|holder").await;
    let digest = digest_of("afc_fixture_live");
    fixtures
        .cli_credential(&user, &tenant, &digest, false)
        .await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::CliCredential, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person { ref subject, live: Liveness::Live, .. })
                if subject.as_str() == "auth0|holder"
        ),
        "the join must carry oidc_subject, never the users row id, got {found:?}"
    );

    fixtures.cleanup().await;
}

/// Revocation is the nullness of the timestamp, not its value.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_revoked_cli_credential_resolves_as_revoked() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.tenant(&identifier(5)).await;
    // A subject of its own: `uq_users_oidc_subject` is unique across the whole
    // table, so the sibling test's "auth0|holder" is not free to reuse here.
    let user = fixtures
        .user(&identifier(6), &tenant, "auth0|holder-revoked")
        .await;
    let digest = digest_of("afc_fixture_revoked");
    fixtures.cli_credential(&user, &tenant, &digest, true).await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::CliCredential, &digest)
        .await
        .expect("a reachable datastore answers");

    assert!(
        matches!(
            found,
            Some(CredentialRecord::Person {
                live: Liveness::Revoked,
                ..
            })
        ),
        "got {found:?}"
    );

    fixtures.cleanup().await;
}

/// A runner token resolves to a machine, carrying its reconciled verdict.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_runner_token_resolves_to_a_machine() {
    let fixtures = Fixtures::create().await;
    let id = identifier(7);
    let digest = digest_of("agt_r_fixture_live");
    fixtures.runner(&id, &digest, ACTIVE, true).await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::RunnerToken, &digest)
        .await
        .expect("a reachable datastore answers");

    match found.expect("the runner was seeded and must resolve") {
        CredentialRecord::Machine {
            runner,
            degraded,
            live,
        } => {
            assert_eq!(runner.as_str(), id);
            assert!(
                degraded,
                "the reconciliation verdict rides the same read, so the lease \
                 gate never re-reads this row"
            );
            assert_eq!(live, Liveness::Live);
        }
        record @ CredentialRecord::Person { .. } => {
            panic!("a runner token holds no tenant authority, got {record:?}")
        }
    }

    fixtures.cleanup().await;
}

/// Every administrative state but `active` bars the runner plane.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_only_an_active_runner_is_live() {
    let fixtures = Fixtures::create().await;
    for (seed, state) in [
        (10, "cordoned"),
        (11, "draining"),
        (12, "drained"),
        (13, "revoked"),
    ] {
        let digest = digest_of(&format!("agt_r_fixture_{state}"));
        fixtures
            .runner(&identifier(seed), &digest, state, false)
            .await;

        let found = Credentials::new(fixtures.database().clone())
            .resolve(CredentialKind::RunnerToken, &digest)
            .await
            .expect("a reachable datastore answers");

        assert!(
            matches!(
                found,
                Some(CredentialRecord::Machine {
                    live: Liveness::Revoked,
                    ..
                })
            ),
            "a {state} runner must not be live, got {found:?}"
        );
    }

    fixtures.cleanup().await;
}

/// A digest nothing matches is `Ok(None)`, in all three stores.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_unmatched_digest_is_none_not_an_error() {
    let fixtures = Fixtures::create().await;
    let directory = Credentials::new(fixtures.database().clone());
    let digest = digest_of("agt_t_nothing_matches_this");

    for kind in [
        CredentialKind::TenantApiKey,
        CredentialKind::CliCredential,
        CredentialKind::RunnerToken,
    ] {
        let found = directory
            .resolve(kind, &digest)
            .await
            .expect("an empty result is not a failure");
        assert!(
            found.is_none(),
            "{kind:?} answered {found:?} for a digest no row carries"
        );
    }

    fixtures.cleanup().await;
}

/// A session token is verified, never looked up.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_session_token_is_not_looked_up() {
    let fixtures = Fixtures::create().await;

    let found = Credentials::new(fixtures.database().clone())
        .resolve(
            CredentialKind::OidcSessionToken,
            &digest_of("header.body.sig"),
        )
        .await
        .expect("there is no store to be unavailable");

    assert!(
        found.is_none(),
        "a session token has no row to find, and asking must not invent one"
    );

    fixtures.cleanup().await;
}

// ── The distinction the trait exists for ────────────────────────────────────

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
    fixtures.drop_table("core.api_keys").await;

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

// ── Rows the schema accepts and this port cannot read ───────────────────────

/// An identifier Postgres stored and `Uuid7` refuses is `Unavailable`.
///
/// `ck_runners_id_uuidv7` checks the version nibble and nothing else, so a
/// UUID with an RFC-4122-invalid variant is storable. Reporting that row as
/// `Ok(None)` would tell a runner its token is unknown — a rejection it counts
/// — when the truth is that its row is corrupt and an operator must look.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_malformed_identifier_is_unavailable() {
    let fixtures = Fixtures::create().await;
    let digest = digest_of("agt_r_fixture_corrupt_id");
    fixtures
        .runner(&identifier_with_bad_variant(40), &digest, ACTIVE, false)
        .await;

    let answer = Credentials::new(fixtures.database().clone())
        .resolve(CredentialKind::RunnerToken, &digest)
        .await;

    assert!(
        answer.is_err(),
        "a row that cannot be read is not a row that is not there, got {answer:?}"
    );

    fixtures.cleanup().await;
}

/// A blank provider subject is `Unavailable`, not a principal with no identity.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_blank_subject_is_unavailable() {
    let fixtures = Fixtures::create().await;
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
