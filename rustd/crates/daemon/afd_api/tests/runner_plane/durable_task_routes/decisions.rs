//! A task run asks the decision model whether its goal is met: one paid call,
//! reserved once and accounted once, whoever repeats it.

use af_wire::{decision::DecisionRequest, usage::LineKind};
use afd_db::test_util::mint_id;
use afd_fleet::test_util::decision::known;
use serde::Deserialize as _;

use super::binding::{Bound, bind};
use super::fixture::BALANCE;
use super::*;

/// The decision verbs' collection under the fixture run.
fn collection(fixture: &Fixture) -> String {
    path(fixture, "decisions")
}

/// A goal check of `bound`'s task from the fixture run.
fn goal(fixture: &Fixture, bound: &Bound) -> Value {
    json!({"operation_id": mint_id(), "fencing_token": fixture.run.fence,
        "source_turn_id": fixture.run.source,
        "assessment": {"kind": "goal", "action_group_id": mint_id()},
        "durable_task": bound.version(), "retry_of": null,
        "evidence": {"checks": "two failed, one passed"}})
}

/// The run's decision calls, their private payloads and their charges.
async fn counts(fixture: &Fixture) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM runner.decision_calls WHERE original_run_id = $1), \
         (SELECT count(*) FROM runner.decision_payloads p JOIN runner.decision_calls c \
            ON c.id = p.decision_call_id WHERE c.original_run_id = $1), \
         (SELECT count(*) FROM billing.usage_lines WHERE work_id = $1 AND kind = $2)",
    )
    .bind(&fixture.run.id)
    .bind(LineKind::Decision.as_str())
    .fetch_one(&mut *fixture.database.acquire().await.expect("a connection"))
    .await
    .expect("the run's decision rows")
}

/// The tenant's balance.
async fn balance(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1")
        .bind(&fixture.owner.tenant)
        .fetch_one(&mut *fixture.database.acquire().await.expect("a connection"))
        .await
        .expect("the tenant's wallet")
}

// Value: protects=`POST …/durable_tasks/decisions` reserves one goal check of
// the run's task with the task's own statements and hands over the decision
// key once, and `PATCH …/decisions/{call_id}` accounts its receipt once;
// fails_when=a repeat reserves or charges again, another runner reaches the
// call, or malformed evidence or a malformed receipt is stored;
// seam=the production router over live Postgres, NATS and the vault.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_goal_decision_is_reserved_and_accounted_once_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let input = goal(&fixture, &bound);
    refuse_scalar_evidence(&fixture, &input).await;

    let request = reserve(&fixture, &input).await;
    let receipt = json!({"fencing_token": fixture.run.fence, "receipt": known(&request)});
    let item = format!("{}/{}", collection(&fixture), request.call_id);
    for (method, route, body) in [
        (Method::POST, collection(&fixture), &input),
        (Method::PATCH, item.clone(), &receipt),
    ] {
        let response = send(
            &fixture.router,
            method,
            &route,
            Some(&fixture.other.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{route}");
        problem(&json_body(response).await, "UZ-RUN-005");
    }
    assert_eq!(
        counts(&fixture).await,
        (1, 1, 0),
        "another runner adds nothing"
    );
    record(&fixture, &item, &receipt, &request.call_id).await;
    fixture.finish().await;
}

/// Evidence that is not an object is refused before any call is reserved.
async fn refuse_scalar_evidence(fixture: &Fixture, input: &Value) {
    for evidence in [Value::Null, json!(42), json!(true)] {
        let mut malformed = input.clone();
        *malformed
            .get_mut("evidence")
            .expect("the input carries evidence") = evidence;
        let refused = call(
            fixture,
            Method::POST,
            &collection(fixture),
            &malformed,
            StatusCode::BAD_REQUEST,
        )
        .await;
        problem(&refused, "UZ-REQ-001");
    }
    assert_eq!(counts(fixture).await, (0, 0, 0));
}

/// Reserves `input` and answers the one-use request it dispatched; a repeat
/// answers the stored call and dispatches nothing.
async fn reserve(fixture: &Fixture, input: &Value) -> DecisionRequest {
    let route = collection(fixture);
    let issued = call(fixture, Method::POST, &route, input, StatusCode::CREATED).await;
    assert_eq!(field(&issued, "/status"), &json!("dispatch"));
    assert_eq!(field(&issued, "/dispatch/api_key"), &json!(fixture.key));
    assert_eq!(
        field(&issued, "/dispatch/assignment/profile/model_id"),
        &json!(fixture.model.model.id)
    );
    let request = DecisionRequest::deserialize(field(&issued, "/dispatch/request"))
        .expect("a typed decision request");
    assert_eq!(field(&issued, "/call_id"), &json!(request.call_id));
    let names: Vec<&str> = request.questions.iter().map(|q| q.name.as_str()).collect();
    assert!(
        names.contains(&"goal.explained"),
        "the check asks the task's own statement: {names:?}"
    );
    let replay = call(fixture, Method::POST, &route, input, StatusCode::OK).await;
    assert_eq!(
        replay,
        json!({"status": "existing", "call_id": request.call_id, "state": "issued"})
    );
    assert_eq!(counts(fixture).await, (1, 1, 0));
    assert_eq!(
        balance(fixture).await,
        BALANCE,
        "a reservation charges nothing"
    );
    request
}

/// A malformed receipt is refused; the known one is accounted once, and its
/// repeat replays the record without a second charge.
async fn record(fixture: &Fixture, item: &str, receipt: &Value, call_id: &str) {
    let mut malformed = receipt.clone();
    *malformed.get_mut("receipt").expect("the receipt body") = json!({"outcome": "provider"});
    let refused = call(
        fixture,
        Method::PATCH,
        item,
        &malformed,
        StatusCode::BAD_REQUEST,
    )
    .await;
    problem(&refused, "UZ-REQ-001");
    assert_eq!(counts(fixture).await, (1, 1, 0));

    for replayed in [false, true] {
        let recorded = call(fixture, Method::PATCH, item, receipt, StatusCode::OK).await;
        assert_eq!(field(&recorded, "/call_id"), &json!(call_id));
        assert_eq!(field(&recorded, "/state"), &json!("known"));
        assert_eq!(field(&recorded, "/replayed"), &json!(replayed));
        assert_eq!(counts(fixture).await, (1, 1, 1), "one charge");
    }
    assert!(
        balance(fixture).await < BALANCE,
        "the known call is charged"
    );
}
