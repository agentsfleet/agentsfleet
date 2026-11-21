//! Each Save owns its private draft, wallet and model.

use super::super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use af_core::id::Uuid7;
use af_core::turn::status::RECORDED;
use afd_auth::Scope;
use afd_db::test_util::{RunnerPolicy, RunnerSeed};
use afd_turns::view::activity::{ACTOR_ACTIVITY, EVENT_TYPE_ACTIVITY};
use axum::Router;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

pub(super) struct Case {
    pub(super) base: Fixture,
    pub(super) router: Router,
    /// The bus every producer under test notifies through, and a test observes.
    pub(super) bus: afd_bus::Handle,
}

impl Case {
    pub(super) async fn create() -> Self {
        Self::with_budgets(afd_turns::Budgets::default()).await
    }

    pub(super) async fn with_budgets(budgets: afd_turns::Budgets) -> Self {
        let base = Fixture::create().await;
        let bus = crate::harness::bus().await;
        let router = base
            .services(&Scope::ALL)
            .with_fleet_queue_budgets(base.db.clone(), bus.clone(), budgets)
            .router();
        Self { base, router, bus }
    }

    pub(super) fn collection(&self) -> String {
        format!("/v1/workspaces/{}/fleets", self.base.workspace)
    }

    pub(super) async fn request(&self) -> (String, Value) {
        self.request_with(json!({})).await
    }

    pub(super) async fn request_with(&self, config: Value) -> (String, Value) {
        let mut body = self.base.request(&mint());
        body.as_object_mut().expect("request").remove("message");
        body["source"]["definition"]["config"]["x-agentsfleet"]
            .as_object_mut()
            .expect("runtime config")
            .extend(config.as_object().expect("config amendments").clone());
        let response = send(
            &self.router,
            Method::POST,
            &self.base.collection(),
            Some(&self.base.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let item = response.headers()[header::LOCATION]
            .to_str()
            .expect("private location")
            .to_owned();
        let tag = response.headers()[header::ETAG]
            .to_str()
            .expect("draft tag")
            .to_owned();
        assert!(
            json_body(response).await.get("review").is_some(),
            "creation includes the current review"
        );
        let patch = json!({"runtime_model_ref":self.base.model,"instance_name":"http-chat"});
        let response = send_with_headers(
            &self.router,
            Method::PATCH,
            &item,
            Some(&self.base.token),
            &patch.to_string(),
            &[(header::IF_MATCH, &tag)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let draft = json_body(response).await;
        assert_eq!(
            draft.pointer("/review/effective/instance_name"),
            Some(&json!("http-chat"))
        );
        (
            item,
            json!({"source":{"kind":"draft","draft_candidate_id":draft["candidate"]["id"]},
            "name":"http-chat", "review_digest":draft["review"]["review_digest"],"start":{"kind":"none"}}),
        )
    }

    pub(super) async fn fund(&self) {
        sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1,$2,now(),now())")
            .bind(&self.base.tenant).bind(crate::setup::FUNDED_NANOS)
            .execute(&mut *self.base.db.acquire().await.expect("wallet connection")).await.expect("fund wallet");
    }

    pub(super) async fn save(&self, key: &str, body: &Value) -> axum::response::Response {
        send_with_headers(
            &self.router,
            Method::POST,
            &self.collection(),
            Some(&self.base.token),
            &body.to_string(),
            &[(header::HeaderName::from_static("idempotency-key"), key)],
        )
        .await
    }

    pub(super) async fn cleanup(self) {
        self.base.cleanup().await;
    }

    /// Seeds a runner and one live run it holds on `fleet`, so a test can
    /// see what archive and delete do to work in flight; answers both.
    pub(super) async fn live_run(&mut self, fleet: &Uuid7) -> (Uuid7, Uuid7) {
        let runner = mint();
        self.base.own_runner(&runner);
        let mut connection = self.base.db.acquire().await.expect("run connection");
        RunnerSeed::new(&runner, RUNNER_STATUS_ACTIVE, RUN_POLICY)
            .insert(&mut *connection)
            .await
            .expect("own runner");
        let run = mint();
        sqlx::query(LIVE_RUN)
            .bind(&run)
            .bind(&runner)
            .bind(fleet)
            .bind(&self.base.workspace)
            .bind(&self.base.tenant)
            .bind(mint())
            .bind(af_core::timing::RUN_TTL.as_secs_f64())
            .bind(RUN_STATUS_ACTIVE)
            .execute(&mut *connection)
            .await
            .expect("live run");
        (runner, run)
    }
}

/// The runner a seeded run belongs to: admitted to no real work.
const RUN_POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};
/// A runner that may hold runs.
const RUNNER_STATUS_ACTIVE: &str = afd_state::sql::RUNNER_STATUS_ACTIVE;
/// A run still in flight.
const RUN_STATUS_ACTIVE: &str = afd_state::sql::RUN_STATUS_ACTIVE;
/// One live run, expiring a run TTL from now.
const LIVE_RUN: &str = "INSERT INTO thread.runs \
    (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at, \
     posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
     status, posted_message_count, created_at, updated_at) \
    VALUES ($1,$2,$3,$4,$5,$6,now(),'self_managed','fixture-provider','fixture-model', \
            now(),1,clock_timestamp() + make_interval(secs => $7),$8,0,now(),now())";

/// Waits for the runner command channel to carry a Cancel for `run`.
pub(super) async fn cancel_heard(commands: &mut afd_bus::Subscription, run: &Uuid7) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let afd_bus::Received::Message(message) =
                commands.recv().await.expect("command channel")
                && let Ok(afd_bus::Command::Cancel { run_id }) =
                    serde_json::from_str::<afd_bus::Command>(&message.payload)
                && &run_id == run
            {
                return;
            }
        }
    })
    .await
    .expect("the runner is told to cancel its run");
}

/// The fleet a saved item's location names.
pub(super) fn fleet_of(item: &str) -> Uuid7 {
    Uuid7::parse(item.rsplit('/').next().expect("fleet id")).expect("fleet id")
}

/// What a seeded run looks like afterwards: still live, and whether a
/// cancel was asked of it.
pub(super) async fn run_state(case: &Case, run: &Uuid7) -> Option<(String, bool)> {
    sqlx::query_as("SELECT status, cancel_requested_at IS NOT NULL FROM thread.runs WHERE id = $1")
        .bind(run)
        .fetch_optional(&mut *case.base.db.acquire().await.expect("run connection"))
        .await
        .expect("run read")
}

pub(super) fn runtime_history(page: &Value, fleet: &str, workspace: &str) -> Vec<Value> {
    history_identities(page, fleet, workspace);
    let rows = page["items"].as_array().expect("complete history page");
    let mut receipts = Vec::new();
    let mut runtime = Vec::new();
    for row in rows {
        if row["event_type"] == EVENT_TYPE_ACTIVITY {
            assert_eq!(row["actor"], ACTOR_ACTIVITY);
            assert_eq!(row["status"], RECORDED);
            assert!(row["response_text"].is_null());
            receipts.push(
                serde_json::from_str::<Value>(row["request_json"].as_str().expect("receipt JSON"))
                    .expect("safe receipt metadata"),
            );
        } else {
            runtime.push(row.clone());
        }
    }
    receipts.sort_by_key(Value::to_string);
    // pin test: literal is the contract
    assert_eq!(
        receipts,
        vec![
            json!({"kind":"setup_ready", "revision":"1"}),
            json!({"kind":"setup_saved", "revision":"1"}),
        ]
    );
    runtime
}

pub(super) fn history_identities(page: &Value, fleet: &str, workspace: &str) -> Vec<String> {
    let rows = page["items"].as_array().expect("complete history page");
    assert!(
        page["next_cursor"].is_null(),
        "the entire history is inspected"
    );
    let mut identities = std::collections::BTreeSet::new();
    for row in rows {
        assert_eq!(row["fleet_id"], fleet);
        assert_eq!(row["workspace_id"], workspace);
        let id = row["turn_id"].as_str().expect("event identity");
        assert!(
            identities.insert(id.to_owned()),
            "unique history identities"
        );
    }
    identities.into_iter().collect()
}
