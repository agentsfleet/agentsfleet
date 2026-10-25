//! Real history and authority are rechecked after the read-only provider count.
use super::{
    super::{attempt::Attempt, fixture::Fixture},
    compaction, fixture,
};
use afd_authoring::{ModelCalls, ProviderRequest, ProviderResponse, Transport, types::Source};
use afd_core::error_code;
use serde_json::{Value, json};
use std::error::Error as _;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Semaphore;

// Keep the selected summary near capacity after geometric source reduction.
const NATIVE_WINDOW: i32 = 36_000;
const PRIOR_EXCHANGES: i64 = 180;

#[derive(Debug)]
struct Provider {
    counted: Semaphore,
    release: Semaphore,
    counts: AtomicUsize,
    paid: AtomicUsize,
    bodies: Mutex<Vec<Value>>,
    status: u16,
}

#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let body: Value = serde_json::from_slice(request.body()).expect("real selected wire");
        let counting = body.get("max_output_tokens").is_none();
        self.bodies.lock().expect("record request").push(body);
        if counting {
            self.counts.fetch_add(1, Ordering::Relaxed);
            self.counted.add_permits(1);
            self.release
                .acquire()
                .await
                .expect("release count")
                .forget();
            let body =
                serde_json::to_vec(&json!({"input_tokens":123})).expect("native count reply");
            return Ok(ProviderResponse {
                status: self.status,
                headers: http::HeaderMap::new(),
                body: Box::pin(futures_util::stream::once(async move {
                    Ok(bytes::Bytes::from(body))
                })),
            });
        }
        self.paid.fetch_add(1, Ordering::Relaxed);
        let summary = json!({"goal":"continue investigation","constraints":[],"decisions":[],
            "completed":[],"remaining":["continue"],"references":["initial-reference"]})
        .to_string();
        let response = json!({"status":"completed","output":[{"type":"message","content":[
            {"type":"output_text","text":summary}]}],"usage":{"input_tokens":123,"output_tokens":1}});
        let (headers, body) =
            crate::provider_wire::body(200, serde_json::to_vec(&response).expect("summary reply"));
        Ok(ProviderResponse {
            status: 200,
            headers,
            body,
        })
    }
}

async fn setup(status: u16) -> (Fixture, Attempt, Arc<Provider>, ModelCalls) {
    let f = Fixture::create().await;
    let inference = json!({"api":"responses","base_url":"https://api.openai.com/v1",
        "dialect":"openai","max_output_tokens":8192});
    sqlx::query(
        "UPDATE model.platform_models SET context_cap_tokens=$3,inference=$2 WHERE id=$1::uuid",
    )
    .bind(f.catalogue.first().expect("owned model").as_str())
    .bind(inference)
    .bind(NATIVE_WINDOW)
    .execute(&mut *f.connection().await)
    .await
    .expect("owned native route");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    compaction::history(&f, &attempt, PRIOR_EXCHANGES).await;
    let provider = Arc::new(Provider {
        counted: Semaphore::new(0),
        release: Semaphore::new(0),
        counts: AtomicUsize::new(0),
        paid: AtomicUsize::new(0),
        bodies: Mutex::default(),
        status,
    });
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    (f, attempt, provider, calls)
}

async fn start(
    f: &Fixture,
    attempt: &Attempt,
    provider: &Provider,
    calls: ModelCalls,
) -> tokio::task::JoinHandle<afd_authoring::Result<afd_authoring::ModelCallResult>> {
    let request = fixture::request(attempt);
    let (runner, lease) = (f.runner.clone(), attempt.lease.clone());
    let task = tokio::spawn(async move { calls.call(&runner, &lease, request).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        provider.counted.acquire(),
    )
    .await
    .expect("native count precedes paid dispatch")
    .expect("count observed")
    .forget();
    assert_eq!(fixture::receipts(f, attempt).await, (0, 0, false));
    assert_eq!(provider.paid.load(Ordering::Relaxed), 0);
    task
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn native_count_releases_authoring_locks_and_fresh_result_can_commit() {
    let (f, attempt, provider, calls) = setup(200).await;
    let task = start(&f, &attempt, &provider, calls).await;
    let includes_source = {
        let bodies = provider.bodies.lock().expect("recorded count");
        bodies
            .first()
            .is_some_and(|body| body.to_string().contains("initial-reference"))
    };
    assert!(includes_source);
    provider.release.add_permits(1);
    let result = task
        .await
        .expect("dispatch task")
        .expect("fresh count allows one paid summary");
    assert!(result.compacted);
    assert_eq!(provider.counts.load(Ordering::Relaxed), 1);
    assert_eq!(provider.paid.load(Ordering::Relaxed), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    let generation: (i64, i64) = sqlx::query_as(
        "SELECT resource_version,(document->>'generation')::bigint FROM memory.context_checkpoints WHERE draft_id=$1::uuid",
    )
    .bind(attempt.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("summary committed");
    assert_eq!(generation, (1, 1));
    f.cleanup().await;
}

#[derive(Clone, Copy)]
enum Change {
    History,
    Stop,
    Credential,
    Rotation,
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn changed_source_stop_and_revoked_credential_during_count_prevent_paid_dispatch() {
    for change in [
        Change::History,
        Change::Stop,
        Change::Credential,
        Change::Rotation,
    ] {
        let (f, attempt, provider, calls) = setup(200).await;
        let task = start(&f, &attempt, &provider, calls).await;
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            mutate(&f, &attempt, change),
        )
        .await
        .expect("count holds no authoring locks");
        provider.release.add_permits(1);
        let error = task.await.expect("dispatch task").expect_err(match change {
            Change::History => "changed history cannot authorize a paid call",
            Change::Stop => "Stop cannot authorize a paid call",
            Change::Credential => "revoked credential cannot authorize a paid call",
            Change::Rotation => "rotated credential cannot authorize a paid call",
        });
        assert_eq!(
            error.code(),
            if matches!(change, Change::Credential | Change::Rotation) {
                error_code::AUTHORING_MODEL
            } else {
                error_code::AUTHORING_FENCE
            }
        );
        assert_no_paid_state(&f, &attempt, &provider).await;
        f.cleanup().await;
    }
}

async fn mutate(f: &Fixture, attempt: &Attempt, change: Change) {
    match change {
        Change::History => {
            sqlx::query("UPDATE authoring.messages SET answer_text='changed during native count' WHERE draft_id=$1::uuid AND sequence=1")
                .bind(attempt.draft.id.as_str()).execute(&mut *f.connection().await).await.expect("source mutation commits");
        }
        Change::Stop => {
            f.drafts
                .cancel(f.access(), &attempt.draft.id, PRIOR_EXCHANGES + 1)
                .await
                .expect("Stop commits while count waits");
        }
        Change::Credential => {
            sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
                .bind(f.model.as_str())
                .execute(&mut *f.connection().await)
                .await
                .expect("revoke selected credential");
        }
        Change::Rotation => rotate(f).await,
    }
}

async fn rotate(f: &Fixture) {
    let kek = afd_crypto::secret::Kek::from_hex(
        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    )
    .expect("fixture encryption key");
    let raw = serde_json::value::RawValue::from_string(
        json!({"provider":"openai","api_key":"rotated-fixture-key"}).to_string(),
    )
    .expect("fixture replacement");
    let body = afd_vault::SecretBody::parse(&raw).expect("replacement credential");
    afd_vault::Vault::new(
        f.db.clone(),
        Arc::new(kek),
        afd_crypto::entropy::Entropy::new(),
    )
    .replace(
        &f.workspace,
        &afd_vault::SecretName::parse("authoring-model").expect("owned secret"),
        &body,
    )
    .await
    .expect("rotation commits while count waits");
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn unavailable_native_count_leaves_no_paid_intent_usage_or_checkpoint() {
    let (f, attempt, provider, calls) = setup(429).await;
    let task = start(&f, &attempt, &provider, calls).await;
    provider.release.add_permits(1);
    let error = task
        .await
        .expect("dispatch task")
        .expect_err("count rate limit aborts admission");
    assert_eq!(error.code(), error_code::AUTHORING_MODEL);
    assert!(
        error
            .source()
            .expect("typed count cause")
            .to_string()
            .contains("429")
    );
    assert_eq!(provider.counts.load(Ordering::Relaxed), 1);
    assert_no_paid_state(&f, &attempt, &provider).await;
    f.cleanup().await;
}

async fn assert_no_paid_state(f: &Fixture, attempt: &Attempt, provider: &Provider) {
    assert_eq!(provider.paid.load(Ordering::Relaxed), 0);
    assert_eq!(fixture::receipts(f, attempt).await, (0, 0, false));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.context_checkpoints WHERE draft_id=$1::uuid",
    )
    .bind(attempt.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("checkpoint census");
    assert_eq!(count, 0);
}
