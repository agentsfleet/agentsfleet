//! Real daemon history, guarded checkpoint publication and receipt accounting.
use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    fixture,
};
use afd_authoring::{Drafts, ProviderRequest, ProviderResponse, Transport, types::Source};
use afd_billing::biller::{
    Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use afd_billing::ledger::LedgerBiller;
use afd_crypto::entropy::Entropy;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

mod paging;

const CHECKPOINT_COUNT: &str =
    "SELECT count(*) FROM memory.context_checkpoints WHERE draft_id=$1::uuid";

#[derive(Debug)]
struct ChangedHistory {
    db: afd_db::Db,
    draft: afd_core::id::Uuid7,
    ledger: LedgerBiller,
}

#[async_trait::async_trait]
impl Biller for ChangedHistory {
    async fn reserve(&self, ask: &Reservation) -> afd_billing::Result<Hold> {
        let hold = self.ledger.reserve(ask).await?;
        // The first preparation has committed and dropped its connection here.
        let written = sqlx::query("UPDATE authoring.messages SET answer_text='changed after measurement' WHERE draft_id=$1::uuid AND sequence=1")
            .bind(self.draft.as_str())
            .execute(&mut *self.db.acquire().await.expect("biller boundary releases the pool"))
            .await.expect("controlled source change between measured funding and dispatch");
        assert_eq!(written.rows_affected(), 1);
        Ok(hold)
    }
    async fn settle(&self, settlement: &Settlement) -> afd_billing::Result<Receipt> {
        self.ledger.settle(settlement).await
    }
    async fn release(&self, work: &WorkKey, why: Release) -> afd_billing::Result<()> {
        self.ledger.release(work, why).await
    }
    async fn balance(&self, payer: &Account) -> afd_billing::Result<Balance> {
        self.ledger.balance(payer).await
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn changed_history_after_funding_cannot_dispatch_the_previously_measured_body() {
    let mut f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    history(&f, &attempt, 180).await;
    f.drafts = Drafts::new(
        f.db.clone(),
        Entropy::new(),
        Arc::new(ChangedHistory {
            db: f.db.clone(),
            draft: attempt.draft.id.clone(),
            ledger: LedgerBiller::on_system_clock(f.db.clone()),
        }),
    );
    let provider = Arc::new(Provider::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let error = calls
        .call(&f.runner, &attempt.lease, fixture::request(&attempt))
        .await
        .expect_err("a funded body cannot authorize changed history");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_FENCE);
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    assert!(provider.0.lock().expect("provider recording").is_empty());
    let checkpoints: i64 = sqlx::query_scalar(CHECKPOINT_COUNT)
        .bind(attempt.draft.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("no new working notes");
    assert_eq!(checkpoints, 0);
    f.cleanup().await;
}

const SUMMARY: &str = r#"{"goal":"finish the original investigation","constraints":[],"decisions":[],"completed":[],"remaining":["continue"],"references":["initial-reference"]}"#;

#[derive(Debug, Default)]
struct Provider(Mutex<Vec<Value>>);
#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let body: Value = serde_json::from_slice(request.body()).expect("real provider wire");
        let summary = body.get("instructions").and_then(Value::as_str)
            == Some(afd_context::SUMMARY_INSTRUCTIONS);
        if summary {
            assert!(
                body.get("tools")
                    .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty))
            );
        } else {
            assert!(body.to_string().contains("initial-reference"));
            assert!(body.to_string().contains("Investigate the checkout"));
        }
        self.0.lock().expect("record requests").push(body);
        let response = json!({"status":"completed", "output":[{"type":"message",
            "content":[{"type":"output_text","text":if summary {SUMMARY} else {"continued answer"}}]}],
            "usage":{"input_tokens":100,"output_tokens":1}});
        let (headers, body) =
            crate::provider_wire::body(200, serde_json::to_vec(&response).expect("response"));
        Ok(ProviderResponse {
            status: 200,
            headers,
            body,
        })
    }
}

async fn history(f: &Fixture, attempt: &Attempt, prior: i64) {
    let mut connection = f.connection().await;
    sqlx::query("UPDATE authoring.messages SET sequence=$2 WHERE draft_id=$1::uuid")
        .bind(attempt.draft.id.as_str())
        .bind(prior + 1)
        .execute(&mut *connection)
        .await
        .expect("move current request");
    sqlx::query("UPDATE authoring.attempts SET message_sequence=$2 WHERE id=$1::uuid")
        .bind(attempt.id.as_str())
        .bind(prior + 1)
        .execute(&mut *connection)
        .await
        .expect("pin current request");
    sqlx::query("UPDATE authoring.drafts SET last_message_sequence=$2 WHERE id=$1::uuid")
        .bind(attempt.draft.id.as_str())
        .bind(prior + 1)
        .execute(&mut *connection)
        .await
        .expect("pin draft sequence");
    for sequence in 1..=prior {
        sqlx::query("INSERT INTO authoring.messages(id,draft_id,sequence,operation_id,status,user_text,answer_text,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3,$1::uuid,'completed',$4,$5,now(),now())")
            .bind(mint().as_str()).bind(attempt.draft.id.as_str()).bind(sequence)
            .bind(format!("initial-reference investigation step {sequence}: {}", "details ".repeat(20)))
            .bind("completed evidence ".repeat(10)).execute(&mut *connection).await.expect("owned complete exchange");
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn repeated_authoring_compaction_restores_notes_and_continues_with_all_181_messages_intact() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=24000 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("force a small physical window");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    history(&f, &attempt, 180).await;
    let provider = Arc::new(Provider::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let mut summaries = 0;
    for index in 1..=8 {
        let mut request = fixture::request(&attempt);
        request.call_index = index;
        let response = calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect("metered physical call");
        if response.compacted {
            summaries += 1;
        } else {
            assert_eq!(response.text, "continued answer");
            break;
        }
    }
    assert!(
        summaries > 1 && summaries < 8,
        "repeated compaction must reach a normal answer"
    );
    assert_saved(&f, &attempt, summaries).await;
    assert_erased(&f, &attempt, summaries + 1).await;
    f.cleanup().await;
}

async fn assert_erased(f: &Fixture, attempt: &Attempt, receipts: i64) {
    f.drafts
        .delete(f.access(), &attempt.draft.id)
        .await
        .expect("creator erases private working state");
    let count: i64 = sqlx::query_scalar(CHECKPOINT_COUNT)
        .bind(attempt.draft.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("private notes census after deletion");
    assert_eq!(count, 0);
    assert_eq!(fixture::receipts(f, attempt).await.0, receipts);
}

async fn assert_saved(f: &Fixture, attempt: &Attempt, summaries: i64) {
    assert_eq!(
        fixture::receipts(f, attempt).await,
        (
            summaries + 1,
            i32::try_from(summaries + 1).expect("bounded dispatch count"),
            false
        )
    );
    let saved: Value = sqlx::query_scalar(
        "SELECT document FROM memory.context_checkpoints WHERE draft_id=$1::uuid",
    )
    .bind(attempt.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("durable private notes");
    assert_eq!(
        saved.get("generation").and_then(Value::as_i64),
        Some(summaries)
    );
    assert_eq!(
        saved.pointer("/summary/goal").and_then(Value::as_str),
        Some("finish the original investigation")
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.messages WHERE draft_id=$1::uuid")
            .bind(attempt.draft.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("original history retained");
    assert_eq!(count, 181);
}
