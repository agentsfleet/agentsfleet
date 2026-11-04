//! A paid decision's HTTP lifecycle uses real ownership, vault and ledger stores.
#![expect(
    clippy::expect_used,
    reason = "native integration fixture preconditions fail loudly"
)]

mod assertions;
mod fixture;
mod setup;

use afd_wire::task::{DecisionReceiptInput, DecisionRecordDisposition, DecisionReservationInput};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::{
    decision_reply::known,
    harness::{code_of, json_body, send},
};
use fixture::Fixture;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn decision_http_replays_never_resend_credentials_or_charge_another_owner() {
    let mut fixture = Fixture::create().await;
    let (input, request) = issue_and_replay(&fixture).await;
    let receipt = DecisionReceiptInput {
        fencing_token: input.fencing_token,
        receipt: known(&request),
    };
    foreign(&fixture, &input, &receipt, &request.call_id).await;
    settle_and_replay(&fixture, &receipt, &request.call_id).await;
    let saved = success(
        &fixture,
        Method::POST,
        &fixture.collection(),
        &serde_json::to_string(&input).expect("saved reservation JSON"),
        StatusCode::OK,
    )
    .await;
    assertions::saved(
        &saved,
        &request,
        &receipt,
        DecisionRecordDisposition::Current,
    );
    fixture.model.disable().await;
    let revoked = success(
        &fixture,
        Method::POST,
        &fixture.collection(),
        &serde_json::to_string(&input).expect("recorded input after revocation"),
        StatusCode::OK,
    )
    .await;
    assertions::saved(
        &revoked,
        &request,
        &receipt,
        DecisionRecordDisposition::Stale,
    );
    assertions::one_charge(&fixture).await;
    fixture.finish().await;
}

async fn issue_and_replay(
    fixture: &Fixture,
) -> (
    DecisionReservationInput,
    afd_wire::decision::DecisionRequest,
) {
    let input = fixture.reservation();
    let initial = success(
        fixture,
        Method::POST,
        &fixture.collection(),
        &serde_json::to_string(&input).expect("reservation JSON"),
        StatusCode::CREATED,
    )
    .await;
    let request = assertions::dispatch(fixture, &initial);
    fixture.delete_key().await;
    let replay = success(
        fixture,
        Method::POST,
        &fixture.collection(),
        &serde_json::to_string_pretty(&input).expect("reordered reservation JSON"),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        replay,
        json!({"status":"existing", "call_id":request.call_id, "state":"issued"})
    );
    assertions::unaccounted(fixture).await;
    (input, request)
}

async fn settle_and_replay(fixture: &Fixture, receipt: &DecisionReceiptInput, call: &str) {
    let recorded = success(
        fixture,
        Method::PATCH,
        &fixture.item(call),
        &serde_json::to_string(receipt).expect("receipt JSON"),
        StatusCode::OK,
    )
    .await;
    assertions::recorded(&recorded, call, false);
    assertions::one_charge(fixture).await;
    let repeated = success(
        fixture,
        Method::PATCH,
        &fixture.item(call),
        &reordered_receipt(receipt),
        StatusCode::OK,
    )
    .await;
    assertions::recorded(&repeated, call, true);
    assertions::one_charge(fixture).await;
}

fn reordered_receipt(receipt: &DecisionReceiptInput) -> String {
    let value = serde_json::to_value(&receipt.receipt).expect("typed provider receipt");
    let result = value
        .get("result")
        .expect("complete fixture provider result");
    format!(
        "{{\n\"receipt\":{{\"result\":{},\"outcome\":\"provider\"}},\n\"fencing_token\":{}\n}}",
        result, receipt.fencing_token
    )
}

async fn success(
    fixture: &Fixture,
    method: Method,
    path: &str,
    body: &str,
    status: StatusCode,
) -> Value {
    let response = send(
        &fixture.router,
        method,
        path,
        Some(&fixture.runner.token),
        body,
    )
    .await;
    assert_eq!(response.status(), status);
    assert_eq!(
        response
            .headers()
            .get(http::header::CACHE_CONTROL)
            .expect("sensitive replies prevent caching"),
        "no-store"
    );
    json_body(response).await
}

async fn foreign(
    fixture: &Fixture,
    input: &DecisionReservationInput,
    receipt: &DecisionReceiptInput,
    call: &str,
) {
    for (method, path, body) in [
        (
            Method::POST,
            fixture.collection(),
            serde_json::to_string(input).expect("existing operation"),
        ),
        (
            Method::PATCH,
            fixture.item(call),
            serde_json::to_string(receipt).expect("existing receipt"),
        ),
    ] {
        let response = send(
            &fixture.router,
            method,
            &path,
            Some(&fixture.other.token),
            &body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(code_of(response).await, "UZ-RUN-005");
        assertions::unaccounted(fixture).await;
    }
}
