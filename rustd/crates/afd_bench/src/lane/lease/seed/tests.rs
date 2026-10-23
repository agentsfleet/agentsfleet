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
    "SELECT id::text, delivered_at IS NOT NULL FROM fleet.admissions WHERE fleet_id = $1::uuid";

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
    let leases = afd_fleet::lease::Leases::new(
        database.clone(),
        queue.clone(),
        afd_crypto::entropy::Entropy::new(),
    );
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
    let delivered: bool = sqlx::query_scalar(
        "SELECT delivered_at IS NOT NULL FROM fleet.admissions WHERE id=$1::uuid",
    )
    .bind(admission)
    .fetch_one(&mut *database.acquire().await.expect("connection"))
    .await
    .expect("durable delivery");
    assert!(delivered);
    sweep::everything(&database, &prefix)
        .await
        .expect("owned fixtures sweep");
}
