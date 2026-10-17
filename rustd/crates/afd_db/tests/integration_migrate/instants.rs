//! Dimension 2.6 — every instant is `TIMESTAMPTZ(3)` and every instant column
//! ends `_at`, both ways round, so a millisecond-precision timestamp can be
//! neither stored as a bare number nor hidden under another name.

use super::*;

/// Every product column, its type, and its fractional-second precision.
const COLUMN_TYPES: &str = "\
SELECT c.table_schema || '.' || c.table_name || '.' || c.column_name,
       c.data_type::text, c.datetime_precision
  FROM information_schema.columns c
  JOIN information_schema.tables t
    ON t.table_schema = c.table_schema AND t.table_name = c.table_name
 WHERE t.table_type = 'BASE TABLE'
   AND c.table_schema NOT IN ('pg_catalog', 'information_schema', 'audit')";

/// The type every instant column carries.
const INSTANT_TYPE: &str = "timestamp with time zone";

/// Milliseconds: what the API, the cursor and `jiff::Timestamp` all agree on.
const INSTANT_PRECISION: i32 = 3;

/// The suffix every instant column ends in.
const INSTANT_SUFFIX: &str = "_at";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_instant_columns_are_timestamptz_ms() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let columns: Vec<(String, String, Option<i32>)> = sqlx::query_as(COLUMN_TYPES)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    let instants = columns
        .iter()
        .filter(|(name, _, _)| name.ends_with(INSTANT_SUFFIX))
        .count();
    assert!(instants > 0, "the catalog read found no `_at` column");

    for (name, data_type, precision) in &columns {
        let named = name.ends_with(INSTANT_SUFFIX);
        let typed = data_type == INSTANT_TYPE;
        assert_eq!(
            named, typed,
            "{name} is {data_type}: an instant ends `_at` and only an instant does"
        );
        if typed {
            assert_eq!(
                *precision,
                Some(INSTANT_PRECISION),
                "{name} stores milliseconds"
            );
        }
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
