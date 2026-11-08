//! Complete native response and financial outcomes for the same HTTP decision.

use af_wire::{
    decision::DecisionRequest,
    task::{DecisionReceipt, DecisionReceiptInput},
    usage::DECISION_SEQ_BASE,
};
use serde::Deserialize as _;
use serde_json::{Value, json};

use super::{
    fixture::Fixture,
    setup::{BALANCE, MESSAGE},
};

pub(super) fn dispatch(fixture: &Fixture, response: &Value) -> DecisionRequest {
    assert_eq!(response.get("status"), Some(&json!("dispatch")));
    assert_eq!(
        response.pointer("/dispatch/api_key"),
        Some(&json!(fixture.key))
    );
    assert_eq!(
        response.pointer("/dispatch/assignment/profile/model_id"),
        Some(&json!(fixture.model.model.id))
    );
    let request = DecisionRequest::deserialize(
        response
            .pointer("/dispatch/request")
            .expect("one-use native request"),
    )
    .expect("typed native request");
    assert_eq!(response.get("call_id"), Some(&json!(request.call_id)));
    assert_eq!(request.state.get("message"), Some(&json!(MESSAGE)));
    assert_eq!(
        request
            .questions
            .iter()
            .map(|q| q.name.as_str())
            .collect::<Vec<_>>(),
        ["intent", "association", "multiple_requests"]
    );
    request
}

pub(super) fn recorded(response: &Value, call: &str, replayed: bool) {
    assert_eq!(
        response,
        &json!({"call_id":call,"state":"known","replayed":replayed,
        "disposition":"current","usage":{"input_tokens":1000,"cached_input_tokens":200,"cache_write_tokens":0,"output_tokens":100},
        "charged_nanos":"3","overage_nanos":"0","reserved_nanos":"0"})
    );
}

pub(super) fn saved(
    response: &Value,
    request: &DecisionRequest,
    receipt: &DecisionReceiptInput,
    disposition: af_wire::task::DecisionRecordDisposition,
) {
    let result = match &receipt.receipt {
        DecisionReceipt::Provider { result } => Some(result),
        _ => None,
    }
    .expect("the fixture supplied a provider result");
    assert_eq!(
        response,
        &json!({"status":"existing","call_id":request.call_id,"state":"known",
        "recorded":{"request":request,"result":result,"disposition":disposition}})
    );
}

pub(super) async fn unaccounted(fixture: &Fixture) {
    assert_eq!(counts(fixture).await, (1, 1, 0));
    assert_eq!(balance(fixture).await, BALANCE);
}

pub(super) async fn unallocated(fixture: &Fixture) {
    assert_eq!(counts(fixture).await, (0, 0, 0));
    assert_eq!(balance(fixture).await, BALANCE);
}

pub(super) async fn retained(fixture: &Fixture) -> (Value, Value) {
    sqlx::query_as("SELECT to_jsonb(c),to_jsonb(p) FROM runner.decision_calls c JOIN runner.decision_payloads p ON p.decision_call_id=c.id WHERE c.original_lease_id=$1::uuid")
        .bind(fixture.lease.id.as_str())
        .fetch_one(&mut *fixture.database.acquire().await.expect("complete native decision observer"))
        .await.expect("retained call and payload")
}

pub(super) async fn one_charge(fixture: &Fixture) {
    assert_eq!(counts(fixture).await, (1, 1, 1));
    assert_eq!(balance(fixture).await, BALANCE - 3);
    let row: (i64,i64,i64,i64,i64,String,String,String,String,String,String) = sqlx::query_as(
        "SELECT seq,input_token_count,cached_input_token_count,output_token_count,charged_nanos,provider,model,tenant_id::text,workspace_id::text,work_id::text,event_id FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2")
        .bind(fixture.lease.id.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_one(&mut *fixture.database.acquire().await.expect("canonical charge observer")).await.expect("one original-owner line");
    assert_eq!(
        row,
        (
            i64::from(DECISION_SEQ_BASE),
            800,
            200,
            100,
            3,
            "typesafe".to_owned(),
            fixture.model.model.model_name.clone(),
            fixture.owner.tenant.to_string(),
            fixture.owner.workspace.to_string(),
            fixture.lease.id.to_string(),
            fixture.lease.source.clone()
        )
    );
}

pub(super) async fn logged_before_relay(fixture: &Fixture) {
    assert_eq!(counts(fixture).await, (1, 1, 1));
    assert_eq!(balance(fixture).await, BALANCE);
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2",
    )
    .bind(fixture.lease.id.as_str())
    .bind(i64::from(DECISION_SEQ_BASE))
    .fetch_one(&mut *fixture.database.acquire().await.expect("relay boundary observer"))
    .await
    .expect("the retained decision usage line");
    assert!(
        !settled,
        "HTTP receipt records usage without charging inline"
    );
}

pub(super) async fn relay_once(fixture: &Fixture) {
    use afd_billing::biller::{Biller as _, WorkKey};
    let work = WorkKey::FleetLease(fixture.lease.id.clone());
    let groups = afd_billing::ledger::unsettled_work(
        &mut *fixture.database.acquire().await.expect("owned relay read"),
        &work,
    )
    .await
    .expect("relay reads only this lease's committed usage");
    assert_eq!(groups.len(), 1, "one retained decision pricing header");
    let ledger = afd_billing::ledger::LedgerBiller::on_system_clock(fixture.database.clone());
    for group in groups {
        let first = ledger
            .settle(&group)
            .await
            .expect("native relay settlement");
        assert_eq!(first.settled_lines, 1);
        assert_eq!(first.charged.as_i64(), 3);
        assert_eq!(first.taken.as_i64(), 3);
        let replay = ledger
            .settle(&group)
            .await
            .expect("lost relay acknowledgement replay");
        assert_eq!(replay, afd_billing::biller::Receipt::default());
    }
}

async fn counts(fixture: &Fixture) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM runner.decision_calls WHERE original_lease_id=$1::uuid),(SELECT count(*) FROM runner.decision_payloads p JOIN runner.decision_calls c ON c.id=p.decision_call_id WHERE c.original_lease_id=$1::uuid),(SELECT count(*) FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2)")
        .bind(fixture.lease.id.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_one(&mut *fixture.database.acquire().await.expect("physical call and receipt observer")).await.expect("complete retained state counts")
}

async fn balance(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1::uuid")
        .bind(fixture.owner.tenant.as_str())
        .fetch_one(&mut *fixture.database.acquire().await.expect("wallet observer"))
        .await
        .expect("native wallet")
}
