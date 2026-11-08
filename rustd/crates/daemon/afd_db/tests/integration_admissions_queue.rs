//! Fresh bootstrap exposes the queue stamps and index to the actual API role.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a failed datastore precondition fails the test"
)]

use afd_db::{Migrator, config::DbRole, test_util::TestDatabase};

/// Type, nullability, default and timestamp precision of each added column.
const COLUMNS: &str = "SELECT column_name::text, data_type::text, is_nullable::text, \
    column_default::text, datetime_precision FROM information_schema.columns \
    WHERE table_schema = 'fleet' AND table_name = 'admissions' \
      AND column_name = ANY($1) ORDER BY column_name";
/// The actual index expression, including its column order and partial predicate.
const INDEX: &str = "SELECT indexdef FROM pg_indexes \
    WHERE schemaname = 'fleet' AND indexname = 'idx_admissions_unsettled'";
/// A zero-row update still checks privileges for all three new columns.
const UPDATE: &str = "UPDATE fleet.admissions \
    SET lease_fence = 1, parked_at = now(), settled_at = now() WHERE false";

#[derive(sqlx::FromRow)]
struct SchemaField {
    column_name: String,
    data_type: String,
    is_nullable: String,
    column_default: Option<String>,
    datetime_precision: Option<i32>,
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_admissions_queue_migration_bootstraps() {
    let lane = TestDatabase::create().await;
    let migrator = lane.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&migrator)
        .await
        .expect("fresh bootstrap");
    let api = lane.open(DbRole::Api, &[]).await;
    let mut connection = api.acquire().await.expect("runtime connection");
    let columns: Vec<SchemaField> = sqlx::query_as(COLUMNS)
        .bind(&["lease_fence", "parked_at", "settled_at"][..])
        .fetch_all(&mut *connection)
        .await
        .expect("runtime sees queue columns");
    let expected = [
        ("lease_fence", "bigint", None),
        ("parked_at", "timestamp with time zone", Some(3)),
        ("settled_at", "timestamp with time zone", Some(3)),
    ];
    assert_eq!(columns.len(), expected.len());
    for (found, (name, kind, precision)) in columns.iter().zip(expected) {
        assert_eq!(
            (
                found.column_name.as_str(),
                found.data_type.as_str(),
                found.is_nullable.as_str()
            ),
            (name, kind, "YES")
        );
        assert!(found.column_default.is_none(), "new stamps start unset");
        assert_eq!(found.datetime_precision, precision);
    }
    let index: String = sqlx::query_scalar(INDEX)
        .fetch_one(&mut *connection)
        .await
        .expect("partial queue index");
    assert!(
        index.contains("(fleet_id, sequence) WHERE (settled_at IS NULL)"),
        "{index}"
    );
    sqlx::query(UPDATE)
        .execute(&mut *connection)
        .await
        .expect("runtime may move queue state");
    drop(connection);
    api.close().await;
    migrator.close().await;
    lane.cleanup().await;
}
