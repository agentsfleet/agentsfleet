//! `migrate` refuses without its own role, and says which knob.
//!
//! The live half — actually applying 47 migrations — is `integration_migrate.rs`
//! in `afd_db`, which owns the migrator and its parity oracle. What is asserted
//! here is the SUBCOMMAND's contract: which knob it reads, which it does not,
//! and what it renders.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_db::Applied;
use agentsfleetd::migrate::{MigrateFailure, summarise};

/// The role migrate reads, which is deliberately not the serving one.
const MIGRATOR_KNOB: &str = "DATABASE_URL_MIGRATOR";

/// The role `serve` reads.
const API_KNOB: &str = "DATABASE_URL_API";

/// Migrate reads the migrator role, and the API role does not satisfy it.
///
/// The two knobs are separate grants: the API role cannot create or drop. A
/// migrate path that fell back to the serving URL would fail at the first
/// `CREATE TABLE` with a permission error that reads like a bug, or — worse —
/// succeed, because someone had given the API role DDL rights to make this
/// work once.
#[tokio::test]
async fn test_migrate_refuses_without_its_own_role() {
    let only_api = MapEnv::from_pairs([(
        API_KNOB,
        "postgres://afd:afd@127.0.0.1:5432/afd?sslmode=disable",
    )]);

    let failure = agentsfleetd::migrate(&only_api)
        .await
        .expect_err("the API role does not satisfy migrate");

    match failure {
        MigrateFailure::Configuration { knob, .. } => {
            assert_eq!(
                knob, MIGRATOR_KNOB,
                "the operator is told which knob to set"
            );
        }
        MigrateFailure::Run(error) => {
            panic!("migrate must refuse before connecting, got a run failure: {error}")
        }
    }
}

/// The refusal names the knob and carries the resolver's own account.
#[tokio::test]
async fn test_a_migrate_refusal_keeps_its_cause() {
    use std::error::Error as _;

    let failure = agentsfleetd::migrate(&MapEnv::default())
        .await
        .expect_err("an empty environment cannot migrate");

    let rendered = failure.to_string();
    assert!(rendered.contains(MIGRATOR_KNOB), "{rendered}");
    assert!(
        failure.source().is_some(),
        "the resolver's error survives as a cause, per the error standard"
    );
}

/// Migrate needs no master key and no Redis.
///
/// A migration job that demanded them would have to be given credentials it has
/// no use for, which is how a job container ends up holding the KEK.
#[tokio::test]
async fn test_migrate_asks_for_no_secrets() {
    let migrator_only = MapEnv::from_pairs([(
        MIGRATOR_KNOB,
        "postgres://afd:afd@127.0.0.1:1/afd?sslmode=disable",
    )]);

    let failure = agentsfleetd::migrate(&migrator_only)
        .await
        .expect_err("nothing is listening on port 1");

    assert!(
        matches!(failure, MigrateFailure::Run(_)),
        "it got past configuration with no key and no queue, and failed on the socket: {failure}"
    );
}

/// The summary names versions, because "3 applied" never answers which three.
#[test]
fn test_the_summary_names_versions_not_counts() {
    let fresh = Applied {
        applied: vec![100, 110, 200],
        skipped: Vec::new(),
        reaped: 0,
    };
    let rendered = summarise(&fresh);
    assert!(rendered.contains("100"), "{rendered}");
    assert!(rendered.contains("200"), "{rendered}");

    let current = Applied {
        applied: Vec::new(),
        skipped: vec![100, 110],
        reaped: 0,
    };
    assert!(
        summarise(&current).contains("already current"),
        "a no-op run says so plainly"
    );

    let reaped = Applied {
        applied: Vec::new(),
        skipped: vec![100],
        reaped: 2,
    };
    assert!(
        summarise(&reaped).contains("reaped 2"),
        "a run that only reaped is not 'already current'"
    );
}
