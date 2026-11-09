//! The identifiers a seeded fleet is given, and why the schema accepts them.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;

use super::{
    KIND_FLEET, KIND_TENANT, KIND_WORKSPACE, SEEDED_AT, identifier, placement_tag, ready_fleet,
};
use crate::fixture::RunPrefix;
use crate::lane::sweep;

/// An index clear of every lane's population, so this fleet is this test's.
const LIVE_INDEX: u64 = 0x2000_0000;

/// The fleet's admission rows, with whether each carries its receipt.
const SELECT_ADMISSIONS: &str =
    "SELECT id, delivered_at IS NOT NULL FROM fleet.admissions WHERE fleet_id = $1";

#[test]
fn test_an_identifier_passes_the_schemas_version_check() {
    let id = identifier(KIND_FLEET, 0).expect("index 0 yields an identifier");

    let groups: Vec<&str> = id.as_str().split('-').collect();
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
    assert!(
        id.as_str()
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
    );
}

#[test]
fn test_the_three_kinds_never_collide_for_one_index() {
    let id = |kind| identifier(kind, 9).expect("index 9 yields an identifier");
    let (t, w, f) = (id(KIND_TENANT), id(KIND_WORKSPACE), id(KIND_FLEET));
    assert_ne!(t, w);
    assert_ne!(w, f);
    assert_ne!(t, f);
}

#[test]
fn test_two_indexes_never_collide_within_a_kind() {
    let fleet = |index| identifier(KIND_FLEET, index).expect("the index yields an identifier");
    assert_ne!(fleet(1), fleet(2));
    assert_ne!(fleet(0), fleet(u32::MAX.into()));
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: run by hand with `cargo test -p afd_bench --all-features -- --ignored` under the lane's environment"]
async fn test_a_seeded_event_stands_on_its_admission_row_and_is_stamped_once() {
    let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
    // The admission row is the proof; the wake it publishes has no reader.
    let bus = afd_bus::test_util::lane()
        .await
        .expect("the lane's NATS: export TEST_NATS_URL as the integration lane does");
    let prefix = RunPrefix::mint();
    let seeded = ready_fleet(
        &database,
        &bus,
        &prefix,
        &placement_tag(&prefix),
        LIVE_INDEX,
        SEEDED_AT,
    )
    .await
    .expect("a ready fleet seeds");

    let mut connection = database.acquire().await.expect("a connection");
    let rows: Vec<(Uuid7, bool)> = sqlx::query_as(SELECT_ADMISSIONS)
        .bind(&seeded.fleet)
        .fetch_all(&mut *connection)
        .await
        .expect("the admissions read");
    let [(admission, delivered)] = rows.as_slice() else {
        panic!("one seeded event stands on exactly one admission row: {rows:?}");
    };
    assert!(!delivered, "seeded work is still waiting");
    drop(connection);
    let runner = super::runner(
        &database,
        &prefix.name("proof-host"),
        &placement_tag(&prefix),
        SEEDED_AT,
    )
    .await
    .expect("fixture runner");
    let leases =
        afd_fleet::lease::Leases::new(database.clone(), bus, af_crypto::entropy::Entropy::new());
    let acquired = leases
        .select(&runner, &[])
        .await
        .expect("claim")
        .expect("work");
    assert_eq!(&acquired.event_id, admission);
    let first = leases
        .record_received(&acquired)
        .await
        .expect("first delivery");
    let again = leases
        .record_received(&acquired)
        .await
        .expect("repeat delivery");
    assert_eq!(first.delivery, afd_fleet::lease::Delivery::First);
    assert_eq!(again.delivery, afd_fleet::lease::Delivery::Repeat);
    let delivered: bool =
        sqlx::query_scalar("SELECT delivered_at IS NOT NULL FROM fleet.admissions WHERE id=$1")
            .bind(admission)
            .fetch_one(&mut *database.acquire().await.expect("connection"))
            .await
            .expect("durable delivery");
    assert!(delivered);
    sweep::everything(&database, &prefix)
        .await
        .expect("owned fixtures sweep");
}
