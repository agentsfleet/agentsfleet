//! The only substituted boundary is the physical provider request.

use super::super::{attempt::Running, mint};
use afd_auth::Scope;
use afd_authoring::{ProviderRequest, ProviderResponse, Transport};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub(super) const CAPABILITY: &str = concat!(
    "0123456789",
    "0123456789",
    "0123456789",
    "0123456789",
    "abc"
);

#[derive(Debug)]
pub(super) struct Provider {
    pub(super) effects: AtomicUsize,
    response: Value,
}

#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let body: Value = serde_json::from_slice(request.body()).expect("fixed provider JSON");
        assert_eq!(body.get("store"), Some(&Value::Bool(false)));
        assert_eq!(body.get("parallel_tool_calls"), Some(&Value::Bool(false)));
        assert!(body.get("capability_token").is_none());
        self.effects.fetch_add(1, Ordering::SeqCst);
        Ok(ProviderResponse {
            status: 200,
            body: serde_json::to_vec(&self.response).expect("bounded response"),
        })
    }
}

pub(super) struct Model {
    pub(super) running: Running,
    pub(super) provider: Arc<Provider>,
    pub(super) path: String,
    pub(super) request: Value,
}

impl Model {
    pub(super) async fn create() -> Self {
        let mut running = Running::create().await;
        let native = running.initialize();
        let proposal = native
            .get("event")
            .and_then(|event| event.get("proposal"))
            .expect("native proposal");
        let arguments = json!({"proposal_json":proposal.to_string()}).to_string();
        let provider = Arc::new(Provider {
            effects: AtomicUsize::new(0),
            response: json!({"status":"completed",
            "output":[{"type":"function_call","name":"propose_setup","call_id":"call_proposal","arguments":arguments}],
            "usage":{"input_tokens":10,"output_tokens":20}}),
        });
        running.router = running
            .f
            .services(&Scope::ALL)
            .with_authoring_transport(Arc::clone(&provider) as Arc<dyn Transport>)
            .router();
        let mut connection = running
            .f
            .db
            .acquire()
            .await
            .expect("owned setup connection");
        sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
            .bind(running.f.tenant.as_str()).bind(crate::authoring::FUNDED_NANOS).execute(&mut *connection).await.expect("owned wallet");
        let attempt: String = sqlx::query_scalar("UPDATE authoring.attempts SET model_token_hash=$2,reserved_nanos=30000000 WHERE draft_id=$1::uuid RETURNING id::text")
            .bind(&running.draft).bind(hex_hash()).fetch_one(&mut *connection).await.expect("bound admitted capability");
        drop(connection);
        let path = format!("{}/model-calls", running.path.trim_end_matches("/activity"));
        let request = json!({"capability_token":CAPABILITY,"attempt_id":attempt,"fence":"1",
            "dispatch_id":mint(),"call_index":1,"request":{"messages":[{"role":"user","text":"Investigate checkout"}],"max_output_tokens":128}});
        Self {
            running,
            provider,
            path,
            request,
        }
    }

    pub(super) async fn receipts(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM authoring.dispatches d JOIN authoring.attempts a ON a.id=d.attempt_id WHERE a.draft_id=$1::uuid")
            .bind(&self.running.draft).fetch_one(&mut *self.running.f.db.acquire().await.expect("receipt read")).await.expect("durable receipt count")
    }
}

fn hex_hash() -> String {
    hex::encode(Sha256::digest(CAPABILITY.as_bytes()))
}
