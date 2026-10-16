//! What the catalog still holds of the storage slot 950 drops.

use afd_db::Db;

/// Relations and `core` columns slot 950 removes, as catalog rows.
const DROPPED_STORAGE: &str = "\
SELECT (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'core' AND c.relname IN ('fleet_library', 'tenant_fleet_library')),
       (SELECT count(*) FROM information_schema.columns
        WHERE table_schema = 'core' AND column_name IN ('source_markdown',
          'trigger_markdown', 'bundle_content_hash', 'bundle_snapshot_key'))";

/// Every catalog object whose definition still names the dropped storage.
const DANGLING: &str = "\
WITH body(text) AS (
  SELECT definition FROM pg_views WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
  UNION ALL SELECT definition FROM pg_matviews
  UNION ALL SELECT p.prosrc FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
  UNION ALL SELECT pg_get_constraintdef(c.oid) FROM pg_constraint c
    JOIN pg_namespace n ON n.oid = c.connamespace
    WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
  UNION ALL SELECT indexdef FROM pg_indexes
    WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
  UNION ALL SELECT pg_get_triggerdef(t.oid) FROM pg_trigger t WHERE NOT t.tgisinternal
)
SELECT count(*) FROM body WHERE text ~* $1";

/// Word-bounded, so `fleet_library_revisions` is not the dropped `fleet_library`.
const DROPPED_NAMES: &str = "\\mfleet_library\\M|tenant_fleet_library|source_markdown|\
trigger_markdown|bundle_content_hash|bundle_snapshot_key";

/// Dropped tables still present, and dropped columns still present.
pub(super) async fn storage(db: &Db) -> (i64, i64) {
    sqlx::query_as(DROPPED_STORAGE)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

/// Views, functions, constraints, indexes and triggers naming the storage.
pub(super) async fn dangling(db: &Db) -> i64 {
    sqlx::query_scalar(DANGLING)
        .bind(DROPPED_NAMES)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}
