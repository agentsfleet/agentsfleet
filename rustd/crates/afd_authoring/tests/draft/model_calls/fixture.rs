//! Each simulated physical effect happens after the test observes its committed intent.

use afd_authoring::{ModelCallInput, ModelCalls, ProviderRequest, ProviderResponse, Transport};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_wire::authoring::{ModelCall, ModelMessage, ModelRequest};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

use super::super::{attempt::Attempt, fixture::Fixture, mint};

pub(super) const CAPABILITY: &str = concat!(
    "0123456789",
    "0123456789",
    "0123456789",
    "0123456789",
    "abc"
);

#[derive(Debug)]
pub(super) struct Provider {
    pub(super) bodies: Mutex<Vec<Value>>,
    pub(super) entered: Semaphore,
    pub(super) release: Semaphore,
    pub(super) blocked: bool,
    pub(super) status: u16,
    pub(super) response: Value,
}

impl Provider {
    pub(super) fn known() -> Self {
        Self {
            bodies: Mutex::new(Vec::new()),
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            blocked: false,
            status: 200,
            response: json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"private answer"}]}],"usage":{"input_tokens":1,"output_tokens":1}}),
        }
    }

    pub(super) fn count(&self) -> usize {
        self.bodies.lock().expect("provider recording lock").len()
    }
    pub(super) async fn arrived(&self) {
        self.entered
            .acquire()
            .await
            .expect("provider effect signalled")
            .forget();
    }
    pub(super) fn resume(&self) {
        self.release.add_permits(1);
    }
}

#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        self.bodies
            .lock()
            .expect("physical effect record")
            .push(serde_json::from_slice(request.body()).expect("daemon JSON"));
        self.entered.add_permits(1);
        if self.blocked {
            self.release
                .acquire()
                .await
                .expect("test releases provider")
                .forget();
        }
        Ok(ProviderResponse {
            status: self.status,
            body: serde_json::to_vec(&self.response).expect("provider response JSON"),
        })
    }
}

pub(super) async fn calls<T: Transport + 'static>(
    f: &Fixture,
    attempt: &Attempt,
    provider: Arc<T>,
) -> ModelCalls {
    let mut connection = f.connection().await;
    sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
        .bind(f.tenant.as_str()).bind(crate::draft::FUNDED_NANOS).execute(&mut *connection).await.expect("owned wallet");
    sqlx::query("UPDATE authoring.attempts SET model_token_hash=$2,reserved_nanos=30000000 WHERE id=$1::uuid")
        .bind(attempt.id.as_str()).bind(hex::encode(Sha256::digest(CAPABILITY.as_bytes())))
        .execute(&mut *connection).await.expect("pin capability and full runtime bound");
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let providers =
        afd_credential::provider::Providers::new(f.db.clone(), Arc::new(kek), Entropy::new());
    ModelCalls::new(f.drafts.clone(), providers, provider)
}

pub(super) fn request(attempt: &Attempt) -> ModelCallInput {
    ModelCall {
        capability_token: CAPABILITY.into(),
        attempt_id: attempt.id.to_string(),
        fence: "1".into(),
        dispatch_id: mint().to_string(),
        call_index: 1,
        request: ModelRequest {
            messages: vec![ModelMessage::User {
                text: "Investigate the checkout".into(),
            }],
            max_output_tokens: 64,
        },
    }
}

pub(super) async fn receipts(f: &Fixture, attempt: &Attempt) -> (i64, i32, bool) {
    sqlx::query_as("SELECT (SELECT count(*) FROM authoring.dispatches WHERE attempt_id=a.id),call_count,usage_unknown FROM authoring.attempts a WHERE id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("durable receipt state")
}
