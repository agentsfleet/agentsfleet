//! Waiting reads use the undelivered admission indexes.

use super::*;

/// The instant the decoy ledger starts at.
const SEEDED_AT: jiff::Timestamp = jiff::Timestamp::constant(1_700_000_000, 0);

/// Both union branches stop at a page before any merge sort.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_queued_read_plans_on_the_undelivered_index() {
    let (database, db, tenant) = private_database().await;
    let mut connection = db.acquire().await.expect("a private connection");
    sqlx::query(SEED_LEDGER)
        .bind(tenant.as_str())
        .bind(jiff_sqlx::ToSqlx::to_sqlx(SEEDED_AT))
        .bind(40_i32)
        .bind(500_i32)
        .execute(&mut *connection)
        .await
        .expect("the ledger history inserts");
    sqlx::query("ANALYZE fleet.admissions")
        .execute(&mut *connection)
        .await
        .expect("statistics refresh");

    for (label, text) in QUEUED_READ_TEXTS {
        let plan = explain_generic(&mut connection, text).await;
        assert!(
            IN_FLIGHT_INDEXES.iter().any(|index| plan.contains(index)),
            "{label}: {plan}"
        );
        assert!(
            plan.lines()
                .any(|line| line.trim_start().starts_with("Index Cond:")
                    && line.contains("fleet_id")),
            "{label}: the fleet bounds the scan: {plan}"
        );
        assert!(!plan.contains("Seq Scan"), "{label}: {plan}");
        assert_bounded_sorts(&mut connection, text).await;
    }

    drop(connection);
    database.cleanup().await;
}

async fn assert_bounded_sorts(connection: &mut sqlx::PgConnection, text: &str) {
    use sqlx::Row as _;
    let row = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (GENERIC_PLAN, FORMAT JSON) {text}"
    )))
    .fetch_one(connection)
    .await
    .expect("generic union plan");
    let plan: serde_json::Value = row.try_get(0).expect("JSON plan");
    assert_sorts(&plan);
}

fn assert_sorts(value: &serde_json::Value) {
    if matches!(
        value["Node Type"].as_str(),
        Some("Sort" | "Incremental Sort")
    ) {
        assert!(
            !unbounded_scan(value, false),
            "sort reads beyond a branch page: {value}"
        );
    }
    if let Some(plans) = value["Plans"].as_array() {
        for child in plans {
            assert_sorts(child);
        }
    } else if let Some(roots) = value.as_array() {
        for root in roots {
            assert_sorts(&root["Plan"]);
        }
    }
}

fn unbounded_scan(value: &serde_json::Value, bounded: bool) -> bool {
    let bounded = bounded || value["Node Type"] == "Limit";
    if matches!(
        value["Relation Name"].as_str(),
        Some("events" | "admissions")
    ) {
        return !bounded;
    }
    value["Plans"]
        .as_array()
        .is_some_and(|children| children.iter().any(|child| unbounded_scan(child, bounded)))
}
