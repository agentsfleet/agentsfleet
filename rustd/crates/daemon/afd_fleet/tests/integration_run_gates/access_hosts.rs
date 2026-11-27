//! Host grants use the same decision card as access requests. The approved
//! host changes only the network policy of future runs.

use af_wire::access_verb::{AccessRequest, Hostname, RedirectEvidence, RefusedTool};
use afd_approval::{Decision, Requested};
use serde_json::{Value, json};

use super::Fixtures;
use super::access::{Live, answer, rows};

const HOST: &str = "support.vendor.example";

async fn request_host(fixtures: &Fixtures, live: &Live) -> Requested {
    let host = Hostname::parse(HOST).expect("a canonical destination");
    let request = AccessRequest {
        fencing_token: live
            .payload
            .get("fencing_token")
            .and_then(Value::as_u64)
            .expect("issued fence"),
        name: None,
        host: Some(host.clone()),
        evidence: Some(RedirectEvidence {
            tool: RefusedTool::WebFetch,
            from_host: Hostname::parse("docs.vendor.example").expect("a redirecting host"),
            to_host: host,
        }),
        scope: None,
    };
    let request = garde::Unvalidated::new(request)
        .validate()
        .expect("a valid host request");
    fixtures
        .plane()
        .request_access(
            &live.ready.runner,
            live.run_id(),
            &request,
            fixtures.enrolled_at(),
        )
        .await
        .expect("the live run asks")
        .requested
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_host_request_raises_one_card_and_waits() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let first = request_host(&fixtures, &live).await;
    assert!(matches!(first, Requested::Raised { .. }));
    let (turn, evidence): (af_core::id::Uuid7, sqlx::types::Json<Value>) =
        sqlx::query_as("SELECT turn_id, evidence FROM fleet.approval_gates WHERE id = $1")
            .bind(first.gate())
            .fetch_one(&mut *fixtures.database.acquire().await.expect("connection"))
            .await
            .expect("decision card");
    assert_eq!(live.ready.turn_id, turn);
    assert_eq!(
        evidence.0,
        json!({"kind":"host","service":HOST,"scope":null,"redirect":{
        "tool":"web_fetch","from_host":"docs.vendor.example","to_host":HOST}})
    );
    assert!(matches!(
        request_host(&fixtures, &live).await,
        Requested::Pending { .. }
    ));
    assert!(matches!(
        live.report_waiting(&fixtures).await,
        afd_fleet::run::Committed::Settled { .. }
    ));
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 1));
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approved_host_joins_the_allowlist_and_opens_no_search() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let raised = request_host(&fixtures, &live).await;
    answer(
        &fixtures,
        &live.ready.fleet,
        raised.gate().expect("the raised card"),
        Decision::Approved,
    )
    .await;
    assert!(matches!(
        request_host(&fixtures, &live).await,
        Requested::Covered { .. }
    ));
    let message: String = sqlx::query_scalar(
        "SELECT request_json->>'message' FROM thread.turns WHERE resumes_turn_id = $1",
    )
    .bind(&live.ready.turn_id)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("connection"))
    .await
    .expect("asking turn resumed");
    assert!(
        message.contains(HOST) && message.contains("web_fetch") && message.contains("approved")
    );
    fixtures
        .store_secret(
            &live.workspace,
            HOST,
            &json!({"token":"fixture","host":"credentials.example"}),
        )
        .await;
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE thread_id = $1 AND request_json->>'service' = $2",
    )
    .bind(&live.ready.fleet)
    .bind(HOST)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("connection"))
    .await
    .expect("credential receipts");
    assert_eq!(receipts, 0);
    let search = af_wire::web::SearchRequest {
        fencing_token: live
            .payload
            .get("fencing_token")
            .and_then(Value::as_u64)
            .expect("issued fence"),
        call_number: 2,
        query: "vendor guides".into(),
    };
    let search = garde::Unvalidated::new(search)
        .validate()
        .expect("valid search");
    let deadline = tokio::time::Instant::now() + af_wire::web::WEB_DEADLINE;
    assert_eq!(
        fixtures
            .plane()
            .web_search(&live.ready.runner, live.run_id(), &search, deadline)
            .await
            .expect_err("a host grant cannot open search")
            .code(),
        af_core::error_code::WEB_NETWORK_RESTRICTED
    );
    let resumed = live
        .next(&fixtures)
        .await
        .expect("the asking fleet resumes");
    assert_eq!(
        resumed.pointer("/policy/network_policy/http_egress"),
        Some(&json!("restricted"))
    );
    assert_eq!(
        resumed.pointer("/policy/network_policy/allow"),
        Some(&json!([HOST]))
    );
    assert!(
        resumed
            .pointer("/policy/secrets_map")
            .and_then(|map| map.get(HOST))
            .is_none()
    );
    fixtures.cleanup().await;
}
