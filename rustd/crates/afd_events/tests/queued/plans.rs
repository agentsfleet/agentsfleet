//! Waiting reads use the undelivered admission indexes.

use super::*;

/// The instant the decoy ledger starts at.
const SEEDED_AT: jiff::Timestamp = jiff::Timestamp::constant(1_700_000_000, 0);

/// Both waiting reads probe an index of in-flight work alone under a generic
/// plan, over a ledger whose history is almost all delivered: the fleet is an
/// index condition, nothing is sorted, and no table is scanned whole.
///
/// The reads order by `(created_at, id)`, the key the merged thread page and
/// its cursor cut on, and `idx_admissions_delivery_lookup` keys exactly that
/// after the fleet, so the order comes off the index.
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
        assert!(
            !plan.contains("Sort"),
            "{label}: the order comes off the index: {plan}"
        );
    }

    drop(connection);
    database.cleanup().await;
}
