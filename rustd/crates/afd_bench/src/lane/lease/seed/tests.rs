//! The identifiers a seeded fleet is given, and why the schema accepts them.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_dragonfly::{DragonflyConfig, DragonflyRole};

use super::{
    KIND_FLEET, KIND_TENANT, KIND_WORKSPACE, SEEDED_AT, identifier, placement_tag, ready_fleet,
};
use crate::fixture::RunPrefix;
use crate::lane::sweep;

/// The lane's Dragonfly URL and its certificate authority, when it serves TLS.
const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// An index clear of every lane's population, so this fleet is this test's.
const LIVE_INDEX: u64 = 0x2000_0000;

/// The fleet's admission rows, with whether each carries its receipt.
const SELECT_ADMISSIONS: &str =
    "SELECT id::text, receipt IS NOT NULL FROM fleet.admissions WHERE fleet_id = $1::uuid";

#[test]
fn test_an_identifier_passes_the_schemas_version_check() {
    let id = identifier(KIND_FLEET, 0);

    let groups: Vec<&str> = id.split('-').collect();
    assert_eq!(groups.len(), 5, "a UUID has five groups, got {id}");
    assert_eq!(
        groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12]
    );
    assert!(
        groups
            .get(2)
            .is_some_and(|version| version.starts_with('7')),
        "the character after the second dash must be 7 or `fleet.fleets` refuses the row: {id}"
    );
    assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
}

#[test]
fn test_the_three_kinds_never_collide_for_one_index() {
    let (t, w, f) = (
        identifier(KIND_TENANT, 9),
        identifier(KIND_WORKSPACE, 9),
        identifier(KIND_FLEET, 9),
    );
    assert_ne!(t, w);
    assert_ne!(w, f);
    assert_ne!(t, f);
}

#[test]
fn test_two_indexes_never_collide_within_a_kind() {
    assert_ne!(identifier(KIND_FLEET, 1), identifier(KIND_FLEET, 2));
    assert_ne!(
        identifier(KIND_FLEET, 0),
        identifier(KIND_FLEET, u32::MAX.into())
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and TEST_DRAGONFLY_URL — run via make test-integration-rustd"]
async fn test_a_seeded_event_stands_on_its_admission_row_and_is_stamped_once() {
    let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
    let url = ProcessEnv
        .get(DRAGONFLY_URL_KNOB)
        .expect("the lane's Dragonfly URL is set");
    let config = DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get(DRAGONFLY_CA_KNOB).map(Into::into));
    let queue = afd_dragonfly::test_util::connect_live(&config)
        .await
        .expect("the lane's Dragonfly answers");
    let prefix = RunPrefix::mint();
    let seeded = ready_fleet(
        &database,
        &queue,
        &prefix,
        &placement_tag(&prefix),
        LIVE_INDEX,
        SEEDED_AT,
    )
    .await
    .expect("a ready fleet seeds");

    let mut connection = database.acquire().await.expect("a connection");
    let rows: Vec<(String, bool)> = sqlx::query_as(SELECT_ADMISSIONS)
        .bind(&seeded.fleet)
        .fetch_all(&mut *connection)
        .await
        .expect("the admissions read");
    let [(admission, receipted)] = rows.as_slice() else {
        panic!("one seeded event stands on exactly one admission row: {rows:?}");
    };
    assert!(receipted, "the ledger recorded the append's receipt");
    let first = stamp(&mut connection, &seeded.fleet, admission).await;
    let again = stamp(&mut connection, &seeded.fleet, admission).await;
    drop(connection);
    sweep::everything(&database, &queue, &prefix)
        .await
        .expect("the run's rows sweep");

    assert_eq!(first, 1, "the delivery stamp meets exactly the seeded row");
    assert_eq!(again, 0, "a stamped row is not stamped twice");
}

/// The lease path's delivery stamp on one admission row, answering how many
/// rows it moved.
async fn stamp(connection: &mut sqlx::PgConnection, fleet: &str, admission: &str) -> u64 {
    sqlx::query(afd_admission::sql::MARK_DELIVERED)
        .bind(fleet)
        .bind(admission)
        .execute(connection)
        .await
        .expect("the stamp runs")
        .rows_affected()
}
