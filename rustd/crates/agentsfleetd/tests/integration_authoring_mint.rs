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
use afd_core::{
    clock,
    error_code::{GRANT_NOT_FOUND, INVALID_REQUEST, RUN_LEASE_NOT_FOUND},
};
use afd_db::test_util::mint_id;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

const MINT: &str = "/v1/runners/me/credentials/mint";
const INTEGRATION: &str = "github";
/// How long a seeded lease stays live, past any request this test makes.
const LIVE_FOR_MS: i64 = 60_000;
/// The bound on every request a case makes to the daemon it booted.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
/// Longer than any identifier the daemon issues, short of the body ceiling.
const OVERLONG_ID_LEN: usize = 4_096;

const AUTHORING: &str = "\
INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,
draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,
lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,
output_tokens,activity_sequence,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7::uuid,1,1,0,$1,'{}'::jsonb,
'active',$8,$8,0,0,false,0,0,0,0,$9,$9)";

const RUNTIME: &str = "\
INSERT INTO fleet.runner_leases(id,runner_id,fleet_id,workspace_id,tenant_id,event_id,receipt,
actor,event_type,event_created_at,posture,provider,model,metered_input_tokens,
metered_cached_tokens,metered_output_tokens,last_metered_at,fencing_token,lease_expires_at,
status,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$1,'1-0','fixture','chat',$6,'platform',
'test','test',0,0,0,$6,1,$7,'active',$6,$6)";

async fn execute(run: &Scenario, statement: &'static str, binds: &[&str], times: [i64; 2]) {
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind((*bind).to_owned());
    }
    query
        .bind(times[0])
        .bind(times[1])
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
    let now = clock::now().as_millis();
    let runner = run.runner_id.as_str();
    let (attempt, authoring_lease, runtime_lease) = (mint_id(), mint_id(), mint_id());
    let (creator, draft) = (mint_id(), mint_id());
    let ids = [
        attempt.as_str(),
        authoring_lease.as_str(),
        runner,
        run.workspace.as_str(),
        run.tenant.as_str(),
        creator.as_str(),
        draft.as_str(),
    ];
    execute(&run, AUTHORING, &ids, [now + LIVE_FOR_MS, now]).await;
    let ids = [
        runtime_lease.as_str(),
        runner,
        run.fleet.as_str(),
        run.workspace.as_str(),
        run.tenant.as_str(),
    ];
    execute(&run, RUNTIME, &ids, [now, now + LIVE_FOR_MS]).await;

    let (status, control) = mint(&http, &run, &runtime_lease).await;
    // Past the lease lookup and stopped one gate later, at the grant.
    assert_eq!(status, 403, "{control}");
    assert_eq!(
        field(&control, "error_code"),
        &json!(GRANT_NOT_FOUND.as_str())
    );
    let (status, live) = mint(&http, &run, &authoring_lease).await;
    assert_eq!(status, 404, "{live}");
    assert_eq!(
        field(&live, "error_code"),
        &json!(RUN_LEASE_NOT_FOUND.as_str())
    );
    assert!(live.get("token").is_none(), "no credential in a refusal");

    sqlx::query(
        "UPDATE fleet.authoring_attempts SET status='cancelled',lease_expires_at=$2 \
         WHERE id=$1::uuid",
    )
    .bind(&attempt)
    .bind(now)
    .execute(&mut *run.booted.database.acquire().await.expect("connection"))
    .await
    .expect("revoke the authoring capability");
    let (status, revoked) = mint(&http, &run, &authoring_lease).await;
    assert_eq!(status, 404, "{revoked}");
    assert_eq!(
        revoked, live,
        "revocation changes nothing a caller can read"
    );

    for (statement, id) in [
        (
            "DELETE FROM fleet.authoring_attempts WHERE id=$1::uuid",
            &attempt,
        ),
        (
            "DELETE FROM fleet.runner_leases WHERE id=$1::uuid",
            &runtime_lease,
        ),
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
    // The lease routes that once handed their path id to a `::uuid` cast:
    // malformed is the caller's mistake, well-formed and unknown is hidden.
    for (suffix, body) in [
        ("renew", json!({})),
        ("activity", json!({"frames": []})),
        ("tool-calls", json!({"fencing_token": 7, "calls": []})),
    ] {
        for (lease, status, code) in [
            ("x".to_owned(), 400, INVALID_REQUEST),
            (mint_id(), 404, RUN_LEASE_NOT_FOUND),
        ] {
            let path = format!("/v1/runners/me/leases/{lease}/{suffix}");
            let response = post(&http, &run, &path, &body).await;
            assert_eq!(response.status().as_u16(), status, "{suffix} {lease}");
            let answer = json(response).await;
            assert_eq!(
                field(&answer, "error_code"),
                &json!(code.as_str()),
                "{suffix} {lease}"
            );
        }
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}
