//! Dimension 2.6 — every instant is `TIMESTAMPTZ(3)` and every instant column
//! ends `_at`, apart from the two price-range names Indy approved in #771.
//! Both directions still enforce the type and precision of every instant.

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

/// The exact #771 names Indy approved on 2026-10-18, scoped to this table.
const PRICE_RANGE_INSTANTS: [&str; 2] = [
    "model.platform_model_prices.effective_from",
    "model.platform_model_prices.effective_until",
];

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
        let named = name.ends_with(INSTANT_SUFFIX) || PRICE_RANGE_INSTANTS.contains(&name.as_str());
        let typed = data_type == INSTANT_TYPE;
        assert_eq!(
            named, typed,
            "{name} is {data_type}: an instant ends `_at` or has an approved price-range name"
        );
        if typed {
            assert_eq!(
                *precision,
                Some(INSTANT_PRECISION),
                "{name} stores milliseconds"
            );
        }
    }
    assert!(
        PRICE_RANGE_INSTANTS
            .iter()
            .all(|expected| columns.iter().any(|(name, _, _)| name == expected))
    );

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
