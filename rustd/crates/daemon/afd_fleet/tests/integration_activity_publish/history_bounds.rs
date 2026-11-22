//! Persisted history keeps its shape even outside the writer; nothing caps
//! how long it grows.

use crate::report_seed::held;
use serde_json::json;
use sqlx::types::Json;

const WRITE: &str =
    "UPDATE thread.turns SET task_activity=$2 WHERE thread_id=$1 AND opened_at IS NOT NULL";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn database_refuses_non_arrays_and_negative_sequences() {
    let run = held().await;
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("API connection");
    for allowed in [json!([]), json!(vec![0; 129]), json!(["x".repeat(119_997)])] {
        let result = sqlx::query(WRITE)
            .bind(&run.fleet)
            .bind(Json(allowed))
            .execute(&mut *connection)
            .await
            .expect("any array is permitted, however long");
        assert_eq!(result.rows_affected(), 1);
    }
    for rejected in [json!({}), json!(null), json!(1)] {
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
            Some("22023")
        );
    }
    let error = sqlx::query(
        "UPDATE thread.turns SET task_activity_sequence=-1 WHERE thread_id=$1 AND opened_at IS NOT NULL",
    )
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
