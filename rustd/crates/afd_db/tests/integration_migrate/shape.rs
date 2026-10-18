//! Dimension 2.1 — migrating an empty database builds exactly the tables and
//! columns `docs/architecture/postgres_schemas.md` signs off, in the nine owned
//! schemas and nowhere else.
//!
//! The design doc is read as the expected value rather than restated here, so
//! the doc and the slots cannot drift apart without this test failing.

use std::collections::BTreeMap;

use super::*;

/// The signed-off layout, embedded at build time.
const DESIGN: &str = include_str!("../../../../../docs/architecture/postgres_schemas.md");

/// The nine owned schemas, plus `audit`, which holds only sqlx's ledger.
const OWNED_SCHEMAS: [&str; 9] = [
    "identity",
    "authoring",
    "fleet",
    "runner",
    "connector",
    "model",
    "billing",
    "vault",
    "memory",
];

/// Every table outside the system schemas, with its columns in any order.
const BUILT_COLUMNS: &str = "\
SELECT c.table_schema || '.' || c.table_name, c.column_name::text
  FROM information_schema.columns c
  JOIN information_schema.tables t
    ON t.table_schema = c.table_schema AND t.table_name = c.table_name
 WHERE t.table_type = 'BASE TABLE'
   AND c.table_schema NOT IN ('pg_catalog', 'information_schema')";

/// Table name → column names, as a set per table.
type Layout = BTreeMap<String, BTreeSet<String>>;

/// The tables the design doc's per-schema tables list.
///
/// A row is a pipe table row whose first cell is the backticked table name and
/// whose third is the backticked column list, under a `## <schema>` heading or
/// naming its schema itself (`vault.secrets`). A parenthesised note after a
/// column (`id (=1)`) is not part of its name.
fn designed() -> Layout {
    let mut layout = Layout::new();
    let mut schema = "";
    for line in DESIGN.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            schema = heading.split_whitespace().next().unwrap_or_default();
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let [_, table, _, columns, ..] = cells.as_slice() else {
            continue;
        };
        let (Some(table), Some(columns)) = (backticked(table), backticked(columns)) else {
            continue;
        };
        let name = if table.contains('.') {
            table.to_owned()
        } else {
            format!("{schema}.{table}")
        };
        let columns = columns
            .split(',')
            .map(|column| {
                column
                    .split(" (")
                    .next()
                    .unwrap_or(column)
                    .trim()
                    .to_owned()
            })
            .collect();
        layout.insert(name, columns);
    }
    layout
}

/// The text between the first pair of backticks, when the cell opens with one.
fn backticked(cell: &str) -> Option<&str> {
    cell.strip_prefix('`')?.split('`').next()
}

/// What the migrated database holds, ledger excluded.
async fn built(connection: &mut PgConnection) -> Layout {
    let rows: Vec<(String, String)> = sqlx::query_as(BUILT_COLUMNS)
        .fetch_all(connection)
        .await
        .unwrap();
    let mut layout = Layout::new();
    for (table, column) in rows {
        if table != "audit._sqlx_migrations" {
            layout.entry(table).or_default().insert(column);
        }
    }
    layout
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_from_empty_builds_nine_owned_schemas() {
    let design = designed();
    assert_eq!(
        design.len(),
        50,
        "the design doc lists 50 tables: {:?}",
        design.keys()
    );

    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();
    let built = built(&mut connection).await;

    let designed_tables: BTreeSet<&String> = design.keys().collect();
    let built_tables: BTreeSet<&String> = built.keys().collect();
    assert_eq!(built_tables, designed_tables, "the tables the slots build");
    for (table, columns) in &design {
        assert_eq!(built.get(table), Some(columns), "the columns of {table}");
    }
    for table in built.keys() {
        let schema = table.split('.').next().unwrap_or_default();
        assert!(
            OWNED_SCHEMAS.contains(&schema),
            "{table} sits outside the owned schemas"
        );
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
