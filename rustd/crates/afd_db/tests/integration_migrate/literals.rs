//! Dimension 2.2 — no schema object carries a vocabulary literal (RULE STS).
//!
//! Every string literal in a CHECK, an index predicate, a column default or a
//! trigger function is read out of the catalog and held to the structural
//! allowlist: the UUID version nibble, `TG_OP` values, and exception text no
//! caller matches on. A literal outside it is a value the application also
//! names, and that pair drifts silently.

use super::*;

/// The schemas whose objects are held to the rule. `audit` holds only sqlx's
/// own ledger, which is not ours to shape.
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

/// Every place a schema object can carry a literal, with a name for each.
const OBJECT_TEXT: &str = "\
SELECT 'check ' || con.conname, pg_get_constraintdef(con.oid)
  FROM pg_constraint con JOIN pg_namespace n ON n.oid = con.connamespace
 WHERE con.contype = 'c' AND n.nspname = ANY($1)
UNION ALL
SELECT 'index ' || i.relname, pg_get_expr(x.indpred, x.indrelid)
  FROM pg_index x
  JOIN pg_class i ON i.oid = x.indexrelid
  JOIN pg_namespace n ON n.oid = i.relnamespace
 WHERE x.indpred IS NOT NULL AND n.nspname = ANY($1)
UNION ALL
SELECT 'default ' || c.relname || '.' || a.attname, pg_get_expr(d.adbin, d.adrelid)
  FROM pg_attrdef d
  JOIN pg_class c ON c.oid = d.adrelid
  JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
  JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE n.nspname = ANY($1)
UNION ALL
SELECT 'function ' || p.proname, p.prosrc
  FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
 WHERE n.nspname = ANY($1)";

/// The literals nothing in the application names.
const STRUCTURAL: [&str; 2] = [
    // The UUID version nibble every `ck_*_id_uuidv7` CHECK compares.
    "7", // A trigger's own operation, which is Postgres's vocabulary, not ours.
    "INSERT",
];

/// The approval-gate guard's two refusals. No caller matches on either.
const GATE_REFUSALS: [&str; 2] = [
    "approval_gates spend is fixed before resolution",
    "approval_gates resolved row is immutable",
];

/// Whether `literal` is exception text: `<table> is append-only` or a gate
/// refusal. No caller matches on any of them.
fn is_exception_text(literal: &str) -> bool {
    literal.ends_with(" is append-only") || GATE_REFUSALS.contains(&literal)
}

/// Every single-quoted literal in `text`, with `''` read as one quote.
fn literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('\'') {
        let mut literal = String::new();
        let mut tail = &rest[open + 1..];
        loop {
            let Some(close) = tail.find('\'') else {
                return found;
            };
            literal.push_str(&tail[..close]);
            tail = &tail[close + 1..];
            if let Some(after) = tail.strip_prefix('\'') {
                literal.push('\'');
                tail = after;
            } else {
                break;
            }
        }
        found.push(literal);
        rest = tail;
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_schema_objects_carry_no_vocabulary_literals() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let objects: Vec<(String, String)> = sqlx::query_as(OBJECT_TEXT)
        .bind(&PRODUCT_SCHEMAS[..])
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert!(
        objects.iter().any(|(name, _)| name.starts_with("check ")),
        "the catalog read found no CHECK; the scan is broken, not the schema"
    );
    for (name, text) in &objects {
        for literal in literals(text) {
            assert!(
                STRUCTURAL.contains(&literal.as_str()) || is_exception_text(&literal),
                "{name} carries the literal '{literal}'"
            );
        }
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}

#[test]
fn test_literals_reads_every_quoted_string() {
    assert_eq!(literals("a = '7'::text AND b = 'it''s'"), ["7", "it's"]);
    assert_eq!(literals("no literal here"), Vec::<String>::new());
    assert_eq!(literals("an 'unterminated"), Vec::<String>::new());
}
