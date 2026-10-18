//! Dimension 5.5 — a runner token resolves to a machine, live only while active.
//!
//! The runner class of the credential directory: the reconciled verdict rides
//! the lookup, every status but `active` bars the runner plane, and a row this
//! port cannot read answers unavailable rather than unknown — a runner counts
//! an unknown token toward its self-termination ceiling.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Liveness};
use afd_state::Credentials;
use afd_state::sql::RUNNER_STATUS_ACTIVE as ACTIVE;

use crate::support::{Fixtures, digest_of, identifier, identifier_with_bad_variant};

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

/// An identifier Postgres stored and `Uuid7` refuses is `Unavailable`.
///
/// `ck_runners_id_uuidv7` checks the version nibble and nothing else, so a
/// UUID with an RFC-4122-invalid variant is storable. Reporting that row as
/// `Ok(None)` would tell a runner its token is unknown — a rejection it counts
/// — when the truth is that its row is corrupt and an operator must look.
///
/// The corrupt row gets a database of its own: on the shared lane it would
/// outlive the test and break every suite that reads all runners.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_malformed_identifier_is_unavailable() {
    let fixtures = Fixtures::create_disposable().await;
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
