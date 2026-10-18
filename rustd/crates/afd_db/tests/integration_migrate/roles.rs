//! What each runtime role may do to every product table (RULE SGR), and that
//! every money column stores an exact integer.
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

use sqlx::AssertSqlSafe;

use super::*;

/// SQLSTATE `insufficient_privilege`: the one refusal a probe may meet.
const INSUFFICIENT_PRIVILEGE: &str = "42501";

/// The schemas the product's tables live in.
const PRODUCT_SCHEMAS: [&str; 9] = [
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
/// Rows the request path edits in place and removes.
const READ_WRITE: &[Verb] = &[Verb::Select, Verb::Insert, Verb::Update, Verb::Delete];
/// Rows edited in place and never removed by a request; a purge cascades them.
const KEPT: &[Verb] = &[Verb::Select, Verb::Insert, Verb::Update];
/// Immutable rows a request may remove: revisions, swept receipts.
const WRITE_ONCE: &[Verb] = &[Verb::Select, Verb::Insert, Verb::Delete];
/// Accounting and replay proof: appended and read, never edited or removed.
const APPEND_ONLY: &[Verb] = &[Verb::Select, Verb::Insert];
/// Trigger-maintained totals: read only.
const READ_ONLY: &[Verb] = &[Verb::Select];
/// The singleton catalogue counter: read and bumped.
const READ_BUMP: &[Verb] = &[Verb::Select, Verb::Update];

/// Every product table, the role that reaches it, and what that role may do, as
/// the slots grant it. No row grants `TRUNCATE`. `connector.channels` grants
/// UPDATE on its rebind columns only, which a whole-row probe does not reach.
const GRANTS: [(&str, &str, &[Verb]); 48] = [
    ("identity.tenants", "api_runtime", READ_WRITE),
    ("identity.workspaces", "api_runtime", WRITE_ONCE),
    ("identity.users", "api_runtime", READ_WRITE),
    ("identity.memberships", "api_runtime", WRITE_ONCE),
    ("identity.invites", "api_runtime", READ_WRITE),
    ("identity.api_keys", "api_runtime", READ_WRITE),
    ("identity.cli_credentials", "api_runtime", READ_WRITE),
    ("identity.user_preferences", "api_runtime", READ_WRITE),
    ("vault.secrets", "api_runtime", READ_WRITE),
    ("model.platform_models", "api_runtime", READ_WRITE),
    ("model.platform_model_prices", "api_runtime", READ_WRITE),
    ("model.catalogue_version", "api_runtime", READ_BUMP),
    ("model.platform_defaults", "api_runtime", READ_WRITE),
    ("model.tenant_models", "api_runtime", READ_WRITE),
    ("model.tenant_selections", "api_runtime", READ_WRITE),
    ("authoring.library_entries", "api_runtime", READ_WRITE),
    ("authoring.library_revisions", "api_runtime", WRITE_ONCE),
    ("fleet.fleets", "api_runtime", READ_WRITE),
    ("fleet.revisions", "api_runtime", WRITE_ONCE),
    ("connector.integration_grants", "api_runtime", KEPT),
    ("connector.installs", "api_runtime", READ_WRITE),
    ("connector.channels", "api_runtime", APPEND_ONLY),
    ("fleet.events", "api_runtime", KEPT),
    ("fleet.tool_calls", "api_runtime", READ_WRITE),
    ("fleet.admissions", "api_runtime", KEPT),
    ("fleet.approval_gates", "api_runtime", KEPT),
    ("fleet.obligations", "api_runtime", KEPT),
    ("fleet.schedules", "api_runtime", READ_WRITE),
    ("fleet.sessions", "api_runtime", READ_WRITE),
    ("fleet.activity_counters", "api_runtime", READ_ONLY),
    ("runner.runners", "api_runtime", READ_WRITE),
    ("runner.leases", "api_runtime", READ_WRITE),
    ("runner.fleet_slots", "api_runtime", KEPT),
    ("runner.events", "api_runtime", WRITE_ONCE),
    ("runner.lifetime_counters", "api_runtime", KEPT),
    ("authoring.drafts", "api_runtime", READ_WRITE),
    ("authoring.candidates", "api_runtime", WRITE_ONCE),
    ("authoring.messages", "api_runtime", READ_WRITE),
    ("authoring.attempts", "api_runtime", KEPT),
    ("authoring.dispatches", "api_runtime", KEPT),
    ("authoring.activity_receipts", "api_runtime", APPEND_ONLY),
    ("authoring.answer_receipts", "api_runtime", WRITE_ONCE),
    ("authoring.replay_receipts", "api_runtime", WRITE_ONCE),
    ("billing.wallets", "api_runtime", READ_WRITE),
    ("billing.fleet_charges", "api_runtime", KEPT),
    ("billing.authoring_settlements", "api_runtime", APPEND_ONLY),
    (
        "billing.authoring_report_receipts",
        "api_runtime",
        APPEND_ONLY,
    ),
    ("memory.entries", "memory_runtime", READ_WRITE),
];

/// Every runtime role. Each table is reached by exactly the one role its row
/// names; every other role is refused every verb.
const RUNTIME_ROLES: [&str; 4] = [
    "api_runtime",
    "memory_runtime",
    "ops_readonly_human",
    "ops_readonly_fleet",
];

/// Every table the product schemas hold, found in the catalog, so a table added
/// without a grant row fails the completeness check.
const PRODUCT_TABLES: &str = "\
SELECT n.nspname || '.' || c.relname AS name
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE c.relkind = 'r' AND n.nspname = ANY($1)
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

/// Every table's grants, probed role by role and verb by verb.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_every_table_grants_exactly_its_role() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    assert_money_is_integral(&mut connection).await;
    assert_every_table_has_a_grant_row(&mut connection).await;
    for (table, owner, granted) in GRANTS {
        for role in RUNTIME_ROLES {
            for verb in EVERY_VERB {
                let allowed = probe(&mut connection, role, table, verb).await;
                let expected = role == owner && granted.contains(&verb);
                assert_eq!(allowed, expected, "{role} {verb:?} on {table}");
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
        "authoring.attempts.reserved_nanos",
        "billing.authoring_settlements.charged_nanos",
        "billing.fleet_charges.charged_nanos",
        "billing.wallets.balance_nanos",
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

/// The catalog's product tables and the grant rows above are the same set.
async fn assert_every_table_has_a_grant_row(connection: &mut PgConnection) {
    let found: BTreeSet<String> = sqlx::query_scalar(PRODUCT_TABLES)
        .bind(&PRODUCT_SCHEMAS[..])
        .fetch_all(&mut *connection)
        .await
        .unwrap()
        .into_iter()
        .collect();
    let declared: BTreeSet<String> = GRANTS
        .iter()
        .map(|(table, _, _)| (*table).to_owned())
        .collect();
    assert_eq!(found, declared, "every product table carries one grant row");
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
        // OVERRIDING SYSTEM VALUE: a `GENERATED ALWAYS` identity column
        // (`sequence`) refuses a copied value before privileges are reached.
        Verb::Insert => {
            format!("INSERT INTO {table} OVERRIDING SYSTEM VALUE SELECT * FROM {table} WHERE false")
        }
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
