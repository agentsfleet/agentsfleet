//! Dimension 1.2 — what each runtime role may do to the tables M227 adds, and
//! that every money column stores an exact integer.
//!
//! The lane connects as the database owner, which meets no grant at all, so a
//! suite that only reads rows proves nothing about what a request may do. Each
//! probe here runs under `SET LOCAL ROLE <role>` inside a savepoint it always
//! rolls back, and asserts what Postgres then does — the method the purge's
//! privilege suite in `afd_fleet_lifecycle` set, never `has_table_privilege`.
//!
//! A probe is a zero-row statement (`WHERE false`). Postgres checks privileges
//! while it plans, so a verb the role lacks is refused on an empty match exactly
//! as on a full one, and a verb it holds changes nothing.

use sqlx::{AssertSqlSafe, PgConnection};

use super::*;

/// SQLSTATE `insufficient_privilege`: the one refusal a probe may meet.
const INSUFFICIENT_PRIVILEGE: &str = "42501";

/// The schemas the product's runtime roles reach.
const PRODUCT_SCHEMAS: [&str; 6] = ["core", "fleet", "billing", "vault", "audit", "memory"];

/// A verb a request might run against a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verb {
    Select,
    Insert,
    Update,
    Delete,
    Truncate,
}

const EVERY_VERB: [Verb; 5] = [
    Verb::Select,
    Verb::Insert,
    Verb::Update,
    Verb::Delete,
    Verb::Truncate,
];
/// Rows the request path edits in place.
const READ_WRITE: &[Verb] = &[Verb::Select, Verb::Insert, Verb::Update, Verb::Delete];
/// Rows written once and never edited: revisions, candidates, receipts.
const WRITE_ONCE: &[Verb] = &[Verb::Select, Verb::Insert, Verb::Delete];
/// Accounting: appended and read, never edited or removed.
const APPEND_ONLY: &[Verb] = &[Verb::Select, Verb::Insert];

/// What `api_runtime` may do to every table schema/935–943 creates, as those
/// slots grant it. No row grants `TRUNCATE`.
const API_RUNTIME_GRANTS: [(&str, &[Verb]); 13] = [
    ("core.authoring_drafts", READ_WRITE),
    ("core.authoring_messages", READ_WRITE),
    ("core.authoring_candidates", WRITE_ONCE),
    ("fleet.authoring_attempts", READ_WRITE),
    ("fleet.authoring_dispatches", READ_WRITE),
    ("fleet.authoring_activity_receipts", WRITE_ONCE),
    ("billing.authoring_settlements", APPEND_ONLY),
    ("core.fleet_libraries", READ_WRITE),
    ("core.fleet_library_revisions", WRITE_ONCE),
    ("core.fleet_definition_revisions", WRITE_ONCE),
    ("core.authoring_replay_receipts", READ_WRITE),
    ("billing.authoring_report_receipts", APPEND_ONLY),
    ("core.authoring_answer_receipts", WRITE_ONCE),
];

/// Runtime roles that must not reach private authoring data in any way.
const UNRELATED_ROLES: [&str; 3] = ["memory_runtime", "ops_readonly_human", "ops_readonly_fleet"];

/// The M227 tables the catalog holds, found by name rather than by the list
/// above, so a table added without a grant row fails the completeness check.
const M227_TABLES: &str = "\
SELECT n.nspname || '.' || c.relname AS name
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE c.relkind = 'r'
   AND (c.relname LIKE 'authoring\\_%'
        OR c.relname IN ('fleet_libraries', 'fleet_library_revisions', 'fleet_definition_revisions'))
 ORDER BY 1";

/// Columns in the product schemas whose type can hold a fraction.
const FRACTIONAL_COLUMNS: &str = "\
SELECT table_schema || '.' || table_name || '.' || column_name AS name
  FROM information_schema.columns
 WHERE table_schema = ANY($1) AND data_type IN ('double precision', 'real', 'numeric')";

/// Every money column, named by its unit, with its stored type.
const NANOS_COLUMNS: &str = "\
SELECT table_schema || '.' || table_name || '.' || column_name AS name, data_type
  FROM information_schema.columns
 WHERE table_schema = ANY($1) AND column_name LIKE '%nanos%'";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_nanos_cutover_and_schema_roles() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let outcome = Migrator::new()
        .run(&db)
        .await
        .expect("an empty database migrates");
    assert_eq!(outcome.applied.len(), MIGRATIONS.len());
    let mut connection = db.acquire().await.unwrap();

    assert_money_is_integral(&mut connection).await;
    assert_every_m227_table_has_a_grant_row(&mut connection).await;
    for (table, granted) in API_RUNTIME_GRANTS {
        for verb in EVERY_VERB {
            let allowed = probe(&mut connection, "api_runtime", table, verb).await;
            assert_eq!(
                allowed,
                granted.contains(&verb),
                "api_runtime {verb:?} on {table}: allowed={allowed}, granted={granted:?}"
            );
        }
        for role in UNRELATED_ROLES {
            for verb in EVERY_VERB {
                assert!(
                    !probe(&mut connection, role, table, verb).await,
                    "{role} must not {verb:?} {table}"
                );
            }
        }
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}

/// No product column can hold a fraction, and every `*nanos*` column — the
/// unit every budget, price, wallet and charge is stored in — is a `bigint`.
async fn assert_money_is_integral(connection: &mut PgConnection) {
    let fractional: Vec<String> = sqlx::query_scalar(FRACTIONAL_COLUMNS)
        .bind(&PRODUCT_SCHEMAS[..])
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert!(fractional.is_empty(), "fractional columns: {fractional:?}");

    let nanos: Vec<(String, String)> = sqlx::query_as(NANOS_COLUMNS)
        .bind(&PRODUCT_SCHEMAS[..])
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    let names: BTreeSet<&str> = nanos.iter().map(|(name, _)| name.as_str()).collect();
    for expected in [
        "fleet.authoring_attempts.reserved_nanos",
        "fleet.authoring_attempts.settled_nanos",
        "billing.authoring_settlements.charged_nanos",
        "billing.tenant_wallet.balance_nanos",
    ] {
        assert!(
            names.contains(expected),
            "{expected} missing from {names:?}"
        );
    }
    for (name, data_type) in &nanos {
        assert_eq!(data_type, "bigint", "{name} stores money as {data_type}");
    }
}

/// The catalog's M227 tables and the grant rows above are the same set.
async fn assert_every_m227_table_has_a_grant_row(connection: &mut PgConnection) {
    let found: BTreeSet<String> = sqlx::query_scalar(M227_TABLES)
        .fetch_all(&mut *connection)
        .await
        .unwrap()
        .into_iter()
        .collect();
    let declared: BTreeSet<String> = API_RUNTIME_GRANTS
        .iter()
        .map(|(table, _)| (*table).to_owned())
        .collect();
    assert_eq!(found, declared, "every M227 table carries one grant row");
}

/// Whether `role` may run `verb` on `table`: `true` when Postgres runs the
/// zero-row statement, `false` when it refuses for want of a privilege. Any
/// other failure is a broken probe and fails the test rather than reading as
/// either answer.
async fn probe(connection: &mut PgConnection, role: &str, table: &str, verb: Verb) -> bool {
    let column: String = sqlx::query_scalar(
        "SELECT attname::text FROM pg_attribute WHERE attrelid = $1::regclass AND attnum = 1",
    )
    .bind(table)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    let statement = match verb {
        Verb::Select => format!("SELECT 1 FROM {table} WHERE false"),
        Verb::Insert => format!("INSERT INTO {table} SELECT * FROM {table} WHERE false"),
        Verb::Update => format!("UPDATE {table} SET {column} = {column} WHERE false"),
        Verb::Delete => format!("DELETE FROM {table} WHERE false"),
        Verb::Truncate => format!("TRUNCATE {table}"),
    };
    // Every interpolated identifier is a constant of this file or a column
    // name the catalog just returned; nothing here is caller input.
    execute(connection, "BEGIN".to_owned()).await;
    execute(connection, format!("SET LOCAL ROLE {role}")).await;
    let outcome = sqlx::query(AssertSqlSafe(statement.clone()))
        .execute(&mut *connection)
        .await;
    execute(connection, "ROLLBACK".to_owned()).await;
    let Err(failure) = outcome else { return true };
    let code = failure
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code);
    assert_eq!(
        code.as_deref(),
        Some(INSUFFICIENT_PRIVILEGE),
        "{role} `{statement}` failed for another reason: {failure}"
    );
    false
}

async fn execute(connection: &mut PgConnection, statement: String) {
    let outcome = sqlx::query(AssertSqlSafe(statement.clone()))
        .execute(&mut *connection)
        .await;
    assert!(outcome.is_ok(), "`{statement}`: {:?}", outcome.err());
}
