//! Real router proposals use an isolated admitted row until dispatch is enabled.

use super::{fixture::Fixture, json_body, mint, send};
use afd_auth::{credential::Presented, directory::Digest};
use afd_core::id::Uuid7;
use afd_db::test_util::{RunnerPolicy, RunnerSeed};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

const ATTEMPT: &str = "\
INSERT INTO authoring.attempts(id,lease_id,runner_id,workspace_id,tenant_id,user_id,
draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,
lease_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,
reported_runtime_ms,last_activity_sequence,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7::uuid,1,1,$1,$8::jsonb,
'active',now()+interval '1 minute',now()+interval '1 minute',0,false,0,0,0,0,now(),now())";

/// The host's isolation: no sandbox and no egress.
const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

pub(super) struct Running {
    pub(super) f: Fixture,
    pub(super) router: Router,
    pub(super) token: String,
    pub(super) path: String,
    pub(super) draft: String,
}

impl Running {
    pub(super) async fn create() -> Self {
        let mut f = Fixture::create().await;
        let runner = mint();
        f.own_runner(&runner);
        let token = format!("agt_r{}{}", mint(), mint()).replace('-', "");
        let router = f.router();
        let mut request = f.request(&mint());
        *request.get_mut("source").expect("source") = json!({"kind":"blank"});
        let created = send(
            &router,
            Method::POST,
            &f.collection(),
            Some(&f.token),
            &request.to_string(),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let draft = json_body(created)
            .await
            .get("id")
            .and_then(Value::as_str)
            .expect("draft id")
            .to_owned();
        let lease = mint();
        let digest = Digest::of(&Presented::new(&token).expect("fixture runner token"));
        seed(&f, &runner, &lease, &draft, digest.as_str()).await;
        Self {
            f,
            router,
            token,
            path: format!("/v1/runners/me/authoring-leases/{lease}/activity"),
            draft,
        }
    }

    pub(super) fn initialize(&self) -> Value {
        let request = self.f.request(&mint());
        json!({"fence":"1","deletion_generation":"0","sequence":"1","activity_sequence":"1",
            "event":{"kind":"proposal","proposal":{"kind":"initialize",
                "definition":request.get("source").and_then(|s| s.get("definition")).expect("native Definition")}}})
    }
}

/// Seeds a runner presenting `digest` and an admitted authoring attempt for
/// `draft` under `lease`, with the draft's pending message marked running.
pub(super) async fn seed(f: &Fixture, runner: &Uuid7, lease: &Uuid7, draft: &str, digest: &str) {
    let mut connection = f.db.acquire().await.expect("seed connection");
    RunnerSeed::new(runner.as_str(), "active", POLICY)
        .token_hash(digest)
        .insert(&mut *connection)
        .await
        .expect("own runner");
    let model = afd_credential::provider::registered_model(
        &mut connection,
        &f.tenant,
        &f.model.to_string(),
    )
    .await
    .expect("registered metadata")
    .expect("owner model exists");
    sqlx::query(ATTEMPT)
        .bind(mint().as_str())
        .bind(lease.as_str())
        .bind(runner.as_str())
        .bind(f.workspace.as_str())
        .bind(f.tenant.as_str())
        .bind(f.owner.as_str())
        .bind(draft)
        .bind(serde_json::to_string(&model).expect("metadata JSON"))
        .execute(&mut *connection)
        .await
        .expect("admitted fixture attempt");
    sqlx::query("UPDATE authoring.messages SET status='running' WHERE draft_id=$1::uuid")
        .bind(draft)
        .execute(&mut *connection)
        .await
        .expect("running request");
}
