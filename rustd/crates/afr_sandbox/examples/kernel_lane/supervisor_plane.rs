//! Fake external daemon endpoints, with the real runner HTTP client and codec.
//!
//! These observations prove runner requests, not daemon datastore effects.

use std::sync::{Arc, mpsc};

use afd_wire::lease::LeasePayload;
use afd_wire::paths::{
    RUNNER_CREDENTIALS_MINT, RUNNER_LEASES, RUNNER_REPORTS, SCHEDULE_RUNS_SUFFIX,
};
use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
use afd_wire::schedule_verb::ScheduleRun;
use afr_supervisor::test_util::{FENCING, FLEET_ID, Healthy, LEASE_ID, LEASE_JSON};
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use axum::{Json, Router};
use libtest_mimic::Failed;
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify};

use crate::supervisor_model::{INSTRUCTIONS, STEER};

/// A fixture credential; never a real integration credential.
pub(crate) const TOKEN: &str = "kernel-fixture-credential";
pub(crate) const GRAFANA: &str = "grafana.example.test";
pub(crate) const SCHEDULE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
pub(crate) const SCHEDULE_EVENT_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805c";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805b";
pub(crate) const PROVIDER_TOKEN: &str = "provider-fixture-credential";
pub(crate) const RUNNER_TOKEN: &str = "agt_r_kernel_fixture";

#[derive(Debug)]
pub(crate) struct Received {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) body: Value,
}

#[derive(Debug)]
struct Plane {
    healthy: Healthy,
    lease: Mutex<Option<Value>>,
    calls: mpsc::Sender<Received>,
    reported: Arc<Notify>,
    fail_mutations: bool,
}

pub(crate) struct Server {
    pub(crate) url: String,
    pub(crate) reported: Arc<Notify>,
    pub(crate) calls: mpsc::Receiver<Received>,
    listener: crate::http_server::Server,
}

impl Server {
    pub(crate) async fn start(
        tools: &[&str],
        repository: bool,
        fail: bool,
    ) -> Result<Self, Failed> {
        let (calls, received) = mpsc::channel();
        let reported = Arc::new(Notify::new());
        let granted_until = now() + 60_000;
        let healthy = Healthy {
            assigned: AssignedPolicy {
                sandbox_tier: SandboxTier::LandlockFull,
                network_policy: NetworkPolicy::DenyAllEgress,
                registry_allowlist: Vec::new(),
                worker_count: 1,
                extra_binds: Vec::new(),
            },
            interval_ms: 50,
            granted_until,
        };
        let plane = Arc::new(Plane {
            healthy,
            lease: Mutex::new(Some(lease(tools, repository, granted_until)?)),
            calls,
            reported: Arc::clone(&reported),
            fail_mutations: fail,
        });
        let app = Router::new().fallback(reply).with_state(plane);
        let listener = crate::http_server::Server::serve(app).await?;
        Ok(Self {
            url: listener.url.clone(),
            reported,
            calls: received,
            listener,
        })
    }

    pub(crate) async fn stop(&mut self) -> Result<(), Failed> {
        self.listener.stop().await?;
        Ok(())
    }
}

async fn reply(
    State(plane): State<Arc<Plane>>,
    method: Method,
    uri: Uri,
    body: Option<Json<Value>>,
) -> (StatusCode, Json<Value>) {
    let path = uri.path();
    let body = body.map_or(Value::Null, |Json(value)| value);
    plane
        .calls
        .send(Received {
            method: method.to_string(),
            path: path.to_owned(),
            body,
        })
        .unwrap_or_else(|_| unreachable!("the trial retains its HTTP transcript"));
    if path == RUNNER_REPORTS {
        plane.reported.notify_one();
    }
    if path == RUNNER_LEASES {
        let lease = plane.lease.lock().await.take();
        return (
            StatusCode::OK,
            Json(json!({ "lease": lease, "retry_after_ms": 50 })),
        );
    }
    let mutation = path.ends_with("/schedules") || path.ends_with("/messages");
    if mutation && plane.fail_mutations {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "fixture unavailable" })),
        );
    }
    (StatusCode::OK, Json(response(&plane, &method, path)))
}

fn response(plane: &Plane, method: &Method, path: &str) -> Value {
    match path {
        RUNNER_CREDENTIALS_MINT => {
            json!(afd_wire::credentials::MintCredentialResponse {
                token: TOKEN.into(),
                expires_at_ms: plane.healthy.granted_until
            })
        }
        _ if path.ends_with("/messages") => json!({ "delivered": true }),
        _ if path.ends_with("/schedules") => json!({ "schedule_id": SCHEDULE_ID }),
        _ if path.ends_with(&format!("/{SCHEDULE_RUNS_SUFFIX}")) => json!(ScheduleRun {
            event_id: SCHEDULE_EVENT_ID.into()
        }),
        _ if path.ends_with("/recall") => json!({
            "memory": [{ "key": "prior", "content": "earlier fleet finding", "category": "core" }],
            "shared": []
        }),
        _ => plane.healthy.reply(method.as_str(), path),
    }
}

fn now() -> i64 {
    afd_core::clock::now().as_millis()
}

fn lease(tools: &[&str], repository: bool, expires: i64) -> Result<Value, Failed> {
    let mut lease: LeasePayload<'_> = serde_json::from_str(LEASE_JSON)?;
    lease.lease_id = LEASE_ID.into();
    lease.lease_expires_at = expires;
    lease.fencing_token = FENCING;
    lease.event.fleet_id = FLEET_ID.into();
    lease.event.workspace_id = WORKSPACE_ID.into();
    lease.event.actor = "steer:user".into();
    lease.event.request_json = json!({ "message": STEER }).to_string().into();
    lease.instructions = INSTRUCTIONS.into();
    lease.policy.tools = tools.iter().map(|name| (*name).to_owned().into()).collect();
    lease.policy.context.tool_window = 32;
    lease.policy.context.context_cap_tokens = 65_536;
    lease.policy.api_key = PROVIDER_TOKEN.into();
    lease.policy.network_policy.allow = vec![GRAFANA.into(), "other.example.test".into()];
    lease.policy.secrets_map = Some(json!({ "grafana": { "host": GRAFANA, "token": TOKEN } }));
    if repository {
        lease.policy.repository_binding = Some(RepositoryBinding {
            repositories: vec!["acme/widget".into()],
            access: RepositoryAccess::Write,
            base_branch: "main".into(),
        });
    }
    Ok(serde_json::to_value(lease)?)
}
