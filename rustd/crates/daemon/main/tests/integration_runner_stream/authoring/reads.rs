//! Assertions read the durable attempt, private result and accounting rows.

use super::fixture::AuthoringFixture;
use af_core::id::Uuid7;
use serde_json::Value;

impl AuthoringFixture {
    pub(super) async fn wait_for_known_receipt(&self) {
        tokio::time::timeout(super::DEADLINE, async {
            loop {
                if self.dispatch().await.1.is_some_and(|usage| {
                    usage.0.get("knowledge").and_then(Value::as_str) == Some("known")
                }) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("accepted provider operation records its usage after disconnect");
    }
    pub(super) async fn dispatch(
        &self,
    ) -> (
        Option<sqlx::types::Json<Value>>,
        Option<sqlx::types::Json<Value>>,
    ) {
        sqlx::query_as("SELECT result,usage FROM authoring.dispatches WHERE attempt_id=$1")
            .bind(Uuid7::parse(&self.lease.attempt_id).expect("attempt id"))
            .fetch_one(
                &mut *self
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("dispatch read connection"),
            )
            .await
            .expect("saved provider receipt")
    }

    pub(super) async fn answer(&self) -> Option<String> {
        sqlx::query_scalar(
            "SELECT answer_text FROM authoring.messages WHERE draft_id=$1 AND sequence=1",
        )
        .bind(Uuid7::parse(&self.lease.draft_id).expect("draft id"))
        .fetch_one(
            &mut *self
                .booted
                .database
                .acquire()
                .await
                .expect("answer read connection"),
        )
        .await
        .expect("saved private message")
    }

    pub(super) async fn charges(&self) -> Vec<(i64, i64, i64, i64)> {
        // `kind DESC` reads runtime, then model, then decision lines: each kind
        // numbers its own lines (M245), so `seq` alone no longer orders them.
        sqlx::query_as("SELECT seq,input_token_count,output_token_count,charged_nanos FROM billing.usage_lines WHERE work_id=$1 ORDER BY kind DESC, seq")
            .bind(Uuid7::parse(&self.lease.attempt_id).expect("attempt id"))
            .fetch_all(&mut *self.booted.database.acquire().await.expect("accounting read connection"))
            .await.expect("durable billing lines")
    }

    pub(super) async fn receipts(&self) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM authoring.dispatches WHERE attempt_id=$1), (SELECT count(*) FROM authoring.activity_receipts WHERE attempt_id=$1), (SELECT count(*) FROM billing.authoring_report_receipts WHERE attempt_id=$1)")
            .bind(Uuid7::parse(&self.lease.attempt_id).expect("attempt id"))
            .fetch_one(&mut *self.booted.database.acquire().await.expect("receipt read connection"))
            .await.expect("scoped receipt counts")
    }

    pub(super) async fn connection(&self) -> Uuid7 {
        sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id=$1")
            .bind(&self.runner)
            .fetch_one(
                &mut *self
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("owner read connection"),
            )
            .await
            .expect("native connection owner")
    }
}
