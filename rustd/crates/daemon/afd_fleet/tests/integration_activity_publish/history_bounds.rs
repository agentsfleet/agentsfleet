//! Persisted history keeps its shape and hard bounds even outside the writer.

use crate::report_seed::held;
use serde_json::json;
use sqlx::types::Json;

const WRITE: &str = "UPDATE fleet.events SET task_activity=$2 WHERE fleet_id=$1::uuid";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn database_refuses_non_arrays_excess_history_and_negative_sequences() {
    let run = held().await;
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("API connection");
    for allowed in [json!([]), json!(vec![0; 128]), json!(["x".repeat(119_996)])] {
        let result = sqlx::query(WRITE)
            .bind(&run.fleet)
            .bind(Json(allowed))
            .execute(&mut *connection)
            .await
            .expect("exact storage boundary is permitted");
        assert_eq!(result.rows_affected(), 1);
    }
    for (rejected, expected) in [
        (json!({}), "22023"),
        (json!(null), "22023"),
        (json!(1), "22023"),
        (json!(vec![0; 129]), "23514"),
        (json!(["x".repeat(119_997)]), "23514"),
    ] {
        let error = sqlx::query(WRITE)
            .bind(&run.fleet)
            .bind(Json(rejected))
            .execute(&mut *connection)
            .await
            .expect_err("invalid history must fail at the database boundary");
        assert_eq!(
            error
                .as_database_error()
                .expect("Postgres refusal")
                .code()
                .as_deref(),
            Some(expected)
        );
    }
    let error =
        sqlx::query("UPDATE fleet.events SET task_activity_sequence=-1 WHERE fleet_id=$1::uuid")
            .bind(&run.fleet)
            .execute(&mut *connection)
            .await
            .expect_err("native sequence cannot become negative");
    assert_eq!(
        error
            .as_database_error()
            .expect("sequence check")
            .code()
            .as_deref(),
        Some("23514")
    );
    drop(connection);
    run.fixtures.cleanup().await;
}
