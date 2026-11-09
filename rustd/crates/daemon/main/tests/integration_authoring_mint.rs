//! An authoring lease carries no runtime credential mint right, live or revoked.
//!
//! The mint route resolves its scope from a runtime lease the presenting runner
//! holds. An authoring attempt is a different lease kind with a model-only
//! capability, so its lease id must find no mint scope on the served route —
//! not while it is live, and not once it is revoked. A runtime lease of the
//! same runner on the same workspace is the control: it gets past the lease
//! lookup, so the authoring refusal is about the lease kind, not the runner.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::e2e::{Scenario, scenario};
use crate::wire::{field, json, post};
use af_core::error_code::{GRANT_NOT_FOUND, INVALID_REQUEST, RUN_LEASE_NOT_FOUND};
use af_core::id::Uuid7;
use afd_db::test_util::{EventKind, FleetEventSeed, mint_id};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

const MINT: &str = "/v1/runners/me/credentials/mint";
const INTEGRATION: &str = "github";
/// The bound on every request a case makes to the daemon it booted.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
/// Longer than any identifier the daemon issues, short of the body ceiling.
const OVERLONG_ID_LEN: usize = 4_096;

const AUTHORING: &str = "\
INSERT INTO authoring.attempts(id,lease_id,runner_id,workspace_id,tenant_id,user_id,
draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,
lease_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,
reported_runtime_ms,last_activity_sequence,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,$7,1,1,$1,'{}'::jsonb,
'active',now()+interval '1 minute',now()+interval '1 minute',0,false,0,0,0,0,now(),now())";

const RUNTIME: &str = "\
INSERT INTO runner.leases(id,runner_id,fleet_id,workspace_id,tenant_id,event_id,
event_created_at,posture,provider,model,last_metered_at,fencing_token,
lease_expires_at,status,
posted_message_count,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,now(),'platform','test',
'test',now(),1,now()+interval '1 minute','active',0,now(),now())";

/// The event the runtime lease executes: a chat a person sent, received.
const EVENT: EventKind<'static> = EventKind {
    actor: "fixture",
    event_type: "chat",
    status: "received",
};

async fn execute(run: &Scenario, statement: &'static str, binds: &[&Uuid7]) {
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query
        .execute(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("seed owned lease row");
}

fn client() -> reqwest::Client {
    afd_identity::direct(DEADLINE)
        .build()
        .expect("a loopback client builds")
}

/// The mint answer, without the per-request correlation id two identical
/// refusals differ in.
async fn mint(http: &reqwest::Client, run: &Scenario, lease: &str) -> (u16, Value) {
    let body = json!({"lease_id": lease, "integration": INTEGRATION, "scope": null});
    let response = post(http, run, MINT, &body).await;
    let status = response.status().as_u16();
    let mut answer = json(response).await;
    if let Some(fields) = answer.as_object_mut() {
        fields.remove("request_id");
    }
    (status, answer)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn authoring_lease_cannot_mint_a_runtime_credential_live_or_revoked() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = client();
    let (attempt, authoring_lease, runtime_lease) = (mint_id(), mint_id(), mint_id());
    seed_leases(&run, &attempt, &authoring_lease, &runtime_lease).await;

    let (status, control) = mint(&http, &run, runtime_lease.as_str()).await;
    // Past the lease lookup and stopped one gate later, at the grant.
    assert_eq!(status, 403, "{control}");
    assert_eq!(
        field(&control, "error_code"),
        &json!(GRANT_NOT_FOUND.as_str())
    );
    let (status, live) = mint(&http, &run, authoring_lease.as_str()).await;
    assert_eq!(status, 404, "{live}");
    assert_eq!(
        field(&live, "error_code"),
        &json!(RUN_LEASE_NOT_FOUND.as_str())
    );
    assert!(live.get("token").is_none(), "no credential in a refusal");

    sqlx::query(
        "UPDATE authoring.attempts SET status='cancelled',lease_expires_at=now() \
         WHERE id=$1",
    )
    .bind(&attempt)
    .execute(&mut *run.booted.database.acquire().await.expect("connection"))
    .await
    .expect("revoke the authoring capability");
    let (status, revoked) = mint(&http, &run, authoring_lease.as_str()).await;
    assert_eq!(status, 404, "{revoked}");
    assert_eq!(
        revoked, live,
        "revocation changes nothing a caller can read"
    );

    for (statement, id) in [
        ("DELETE FROM authoring.attempts WHERE id=$1", &attempt),
        ("DELETE FROM runner.leases WHERE id=$1", &runtime_lease),
    ] {
        sqlx::query(statement)
            .bind(id)
            .execute(&mut *run.booted.database.acquire().await.expect("connection"))
            .await
            .expect("remove owned lease row");
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_lease_id_that_is_not_an_identifier_is_a_bad_request_not_a_server_error() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = client();
    for malformed in ["x".to_owned(), "a".repeat(OVERLONG_ID_LEN)] {
        let (status, answer) = mint(&http, &run, &malformed).await;
        assert_eq!(status, 400, "{} chars: {answer}", malformed.len());
        assert_eq!(
            field(&answer, "error_code"),
            &json!(INVALID_REQUEST.as_str())
        );
    }
    // The retained tool-call route validates its identifier before looking up a lease.
    for (lease, status, code) in [
        ("x".to_owned(), 400, INVALID_REQUEST),
        (mint_id().to_string(), 404, RUN_LEASE_NOT_FOUND),
    ] {
        let path = format!("/v1/runners/me/leases/{lease}/tool-calls");
        let response = post(
            &http,
            &run,
            &path,
            &json!({"fencing_token": 7, "calls": []}),
        )
        .await;
        assert_eq!(response.status().as_u16(), status, "tool-calls {lease}");
        let answer = json(response).await;
        assert_eq!(
            field(&answer, "error_code"),
            &json!(code.as_str()),
            "tool-calls {lease}"
        );
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn seed_leases(
    run: &Scenario,
    attempt: &Uuid7,
    authoring_lease: &Uuid7,
    runtime_lease: &Uuid7,
) {
    let (creator, draft) = (mint_id(), mint_id());
    let ids = [
        attempt,
        authoring_lease,
        &run.runner_id,
        &run.workspace,
        &run.tenant,
        &creator,
        &draft,
    ];
    execute(run, AUTHORING, &ids).await;
    // The runtime lease executes an event of its own id.
    FleetEventSeed::new(&run.fleet, &run.workspace, runtime_lease, EVENT)
        .insert(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the runtime lease's event seeds");
    let ids = [
        runtime_lease,
        &run.runner_id,
        &run.fleet,
        &run.workspace,
        &run.tenant,
        runtime_lease,
    ];
    execute(run, RUNTIME, &ids).await;
}
