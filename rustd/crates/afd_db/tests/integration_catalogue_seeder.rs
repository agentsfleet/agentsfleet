//! The operational seeder against the registered schema and real Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::panic,
    reason = "a seeder the lane cannot run fails the test loudly"
)]

use std::path::Path;
use std::time::Duration;

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use tokio::process::Command;

const BUN: &str = "bun";
const CATALOGUE_DATABASE_URL: &str = "TEST_CATALOGUE_DATABASE_URL";
const TIMEOUT: Duration = Duration::from_secs(120);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_catalogue_seeder_uses_registered_schema_and_rolls_back_failures() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let migrated = Migrator::new().run(&db).await;
    db.close().await;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = if migrated.is_ok() {
        tokio::time::timeout(
            TIMEOUT,
            Command::new(BUN)
                .arg("test")
                .arg("--coverage")
                .arg("scripts/model-catalogue")
                .env(CATALOGUE_DATABASE_URL, database.url())
                .current_dir(root)
                .kill_on_drop(true)
                .output(),
        )
        .await
    } else {
        database.cleanup().await;
        panic!("canonical catalogue migration failed: {migrated:?}");
    };
    database.cleanup().await;
    let output = output.unwrap_or_else(|failure| panic!("seeder suite timed out: {failure}"));
    let output = output.unwrap_or_else(|failure| panic!("Bun must be installed: {failure}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}{stderr}");
    assert!(
        output.status.success(),
        "real catalogue seeder suite failed"
    );
    assert!(
        tally(&stderr, "pass") > 0,
        "the seeder suite ran no passing tests"
    );
    // Every database case is `describe.skipIf(!TEST_CATALOGUE_DATABASE_URL)`,
    // so a URL that never reaches Bun skips them all and the suite still
    // exits 0 on the cases that need no database.
    assert_eq!(
        tally(&stderr, "skip"),
        0,
        "the seeder suite skipped cases, so the database cases did not run"
    );
}

/// The count Bun's closing summary gives `label` (`" 81 pass"`, `" 22 skip"`),
/// zero when the summary has no such line: Bun omits a zero skip count.
fn tally(summary: &str, label: &str) -> u64 {
    summary
        .lines()
        .filter_map(|line| line.trim().split_once(' '))
        .find(|(_, named)| *named == label)
        .and_then(|(count, _)| count.parse().ok())
        .unwrap_or(0)
}
