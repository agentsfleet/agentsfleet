//! An authoring run carries no runtime credential mint right, live or revoked.
//!
//! The mint route resolves its scope from a runtime run the presenting runner
//! holds. An authoring attempt is a different run kind with a model-only
//! capability, so its run id must find no mint scope on the served route —
//! not while it is live, and not once it is revoked. A runtime run of the
//! same runner on the same workspace is the control: it gets past the run
//! lookup, so the authoring refusal is about the run kind, not the runner.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::e2e::{Scenario, scenario, scenario_with};
use crate::wire::{field, json, post};
use af_core::error_code::{GRANT_NOT_FOUND, INVALID_REQUEST, RUN_NOT_FOUND};
use af_core::id::Uuid7;
use afd_db::test_util::{EventKind, FleetEventSeed, mint_id};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

const MINT: &str = "/v1/runners/me/credentials/mint";
const INTEGRATION: &str = "github";
/// The runtime control declares the integration whose standing grant is absent.
const FLEET_CONFIG: &str = r#"{"name":"credential-isolation","x-agentsfleet":{"triggers":[],"tools":[],"credentials":["github"],"budget":{"daily_nanos":"1000000000"}}}"#;
/// The bound on every request a case makes to the daemon it booted.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
/// Longer than any identifier the daemon issues, short of the body ceiling.
const OVERLONG_ID_LEN: usize = 4_096;

const AUTHORING: &str = "\
INSERT INTO setup.attempts(id,run_id,runner_id,workspace_id,tenant_id,user_id,
draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,
run_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,
reported_runtime_ms,last_activity_sequence,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,$7,1,1,$1,'{}'::jsonb,
'active',now()+interval '1 minute',now()+interval '1 minute',0,false,0,0,0,0,now(),now())";

const RUNTIME: &str = "\
INSERT INTO thread.runs(id,runner_id,fleet_id,workspace_id,tenant_id,event_id,
event_created_at,posture,provider,model,last_metered_at,fencing_token,
run_expires_at,status,setup_revision_id,
posted_message_count,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,now(),'platform','test',
'test',now(),1,now()+interval '1 minute','active',
(SELECT head_revision_id FROM fleet.fleets WHERE id=$3),0,now(),now())";

/// The event the runtime run executes: a chat a person sent, received.
const EVENT: EventKind<'static> = EventKind {
    actor: "fixture",
    event_type: "chat",
    status: "received",
};

async fn execute(scene: &Scenario, statement: &'static str, binds: &[&Uuid7]) {
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query
        .execute(&mut *scene.booted.database.acquire().await.expect("connection"))
        .await
        .expect("seed owned run row");
}

fn client() -> reqwest::Client {
    afd_identity::direct(DEADLINE)
        .build()
        .expect("a loopback client builds")
}

/// The mint answer, without the per-request correlation id two identical
/// refusals differ in.
async fn mint(http: &reqwest::Client, scene: &Scenario, run: &str) -> (u16, Value) {
    let body = json!({"run_id": run, "integration": INTEGRATION, "scope": null});
    let response = post(http, scene, MINT, &body).await;
    let status = response.status().as_u16();
    let mut answer = json(response).await;
    if let Some(fields) = answer.as_object_mut() {
        fields.remove("request_id");
    }
    (status, answer)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn authoring_run_cannot_mint_a_runtime_credential_live_or_revoked() {
    let mut supervisor = Supervisor::new();
    let scene = scenario_with(&mut supervisor, None, &[], FLEET_CONFIG).await;
    let http = client();
    let (attempt, authoring_run, runtime_run) = (mint_id(), mint_id(), mint_id());
    seed_runs(&scene, &attempt, &authoring_run, &runtime_run).await;

    let (status, control) = mint(&http, &scene, runtime_run.as_str()).await;
    // Past the run lookup and stopped one gate later, at the grant.
    assert_eq!(status, 403, "{control}");
    assert_eq!(
        field(&control, "error_code"),
        &json!(GRANT_NOT_FOUND.as_str())
    );
    let (status, live) = mint(&http, &scene, authoring_run.as_str()).await;
    assert_eq!(status, 404, "{live}");
    assert_eq!(field(&live, "error_code"), &json!(RUN_NOT_FOUND.as_str()));
    assert!(live.get("token").is_none(), "no credential in a refusal");

    sqlx::query(
        "UPDATE setup.attempts SET status='cancelled',run_expires_at=now() \
         WHERE id=$1",
    )
    .bind(&attempt)
    .execute(&mut *scene.booted.database.acquire().await.expect("connection"))
    .await
    .expect("revoke the authoring capability");
    let (status, revoked) = mint(&http, &scene, authoring_run.as_str()).await;
    assert_eq!(status, 404, "{revoked}");
    assert_eq!(
        revoked, live,
        "revocation changes nothing a caller can read"
    );

    for (statement, id) in [
        ("DELETE FROM setup.attempts WHERE id=$1", &attempt),
        ("DELETE FROM thread.runs WHERE id=$1", &runtime_run),
    ] {
        sqlx::query(statement)
            .bind(id)
            .execute(&mut *scene.booted.database.acquire().await.expect("connection"))
            .await
            .expect("remove owned run row");
    }
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_run_id_that_is_not_an_identifier_is_a_bad_request_not_a_server_error() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    let http = client();
    for malformed in ["x".to_owned(), "a".repeat(OVERLONG_ID_LEN)] {
        let (status, answer) = mint(&http, &scene, &malformed).await;
        assert_eq!(status, 400, "{} chars: {answer}", malformed.len());
        assert_eq!(
            field(&answer, "error_code"),
            &json!(INVALID_REQUEST.as_str())
        );
    }
    // The retained tool-call route validates its identifier before looking up a run.
    for (run, status, code) in [
        ("x".to_owned(), 400, INVALID_REQUEST),
        (mint_id().to_string(), 404, RUN_NOT_FOUND),
    ] {
        let path = format!("/v1/runners/me/runs/{run}/tool-calls");
        let response = post(
            &http,
            &scene,
            &path,
            &json!({"fencing_token": 7, "calls": []}),
        )
        .await;
        assert_eq!(response.status().as_u16(), status, "tool-calls {run}");
        let answer = json(response).await;
        assert_eq!(
            field(&answer, "error_code"),
            &json!(code.as_str()),
            "tool-calls {run}"
        );
    }
    supervisor.shutdown().await;
    scene.cleanup().await;
}

async fn seed_runs(scene: &Scenario, attempt: &Uuid7, authoring_run: &Uuid7, runtime_run: &Uuid7) {
    let (creator, draft) = (mint_id(), mint_id());
    let ids = [
        attempt,
        authoring_run,
        &scene.runner_id,
        &scene.workspace,
        &scene.tenant,
        &creator,
        &draft,
    ];
    execute(scene, AUTHORING, &ids).await;
    // The runtime run executes an event of its own id.
    FleetEventSeed::new(&scene.fleet, &scene.workspace, runtime_run, EVENT)
        .insert(&mut *scene.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the runtime run's event seeds");
    let ids = [
        runtime_run,
        &scene.runner_id,
        &scene.fleet,
        &scene.workspace,
        &scene.tenant,
        runtime_run,
    ];
    execute(scene, RUNTIME, &ids).await;
}
