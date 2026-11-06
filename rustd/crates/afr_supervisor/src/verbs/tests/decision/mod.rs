//! Native reservations must become durable receipts even when no request can be sent.

mod loop_boundary;

use super::{FENCING, FencedVerbs, Host, LEASE_ID};
use crate::client::Verb;
use crate::test_support::{Answer, drain, json, plane};
use afd_core::id::Uuid7;
use afd_wire::task::{DecisionCallState, DecisionReservationInput};
use afr_agent::{LeaseVerbs as _, Unanswered};

const CALL: &str = "019f0000-0000-7000-8000-000000000001";

fn input() -> DecisionReservationInput {
    DecisionReservationInput {
        operation_id: CALL.to_owned(),
        fencing_token: 0,
        source_admission_id: CALL.to_owned(),
        assessment: afd_wire::task::DecisionAssessment::Message,
        task: None,
        retry_of: None,
        evidence: serde_json::json!({}),
    }
}

fn dispatch(api: &str, purpose: &str) -> serde_json::Value {
    let threshold = serde_json::json!({"probability_bps":9000,"confidence_bps":9000});
    serde_json::json!({"status":"dispatch","call_id":CALL,"dispatch":{
        "assessment_id":CALL,"attempt":0,"api_key":"fixture-secret-never-sent",
        "expires_at":afd_core::clock::after(afd_core::clock::now(),std::time::Duration::from_secs(30)),
        "assignment":{"revision_id":CALL,"profile":{
            "model_id":CALL,"revision":"reviewed-1","api":api,
            "endpoint":"https://decision.invalid/v1","model":"fixture-model",
            "accepted_models":["fixture-model"],"capabilities":{"noul":true,"choice":true,"score":true},
            "input_max_bytes":24576
        },"intent":{"questions_revision":"dt-routing-v1","intent":threshold,
            "association":threshold,"single_request_max_bps":2000,"multiple_request_min_bps":8000},
        "goal":{"questions_revision":"dt-goal-v1","statement_min_bps":9000,
            "continue_action":threshold,"wait":threshold,"done":threshold}},
        "request":{"call_id":CALL,"purpose":purpose,"state":{"task":{"task_id":CALL},"source_admission_id":CALL},
            "questions":[{"name":"matches","question":{"type":"noul","instructions":"Does the evidence match?",
                "criteria":{"true":"It matches","false":"It does not match"}}}]}
    }})
}

fn acknowledged() -> serde_json::Value {
    serde_json::json!({"call_id":CALL,"state":"not_sent","replayed":false,
        "disposition":"unusable","charged_nanos":"0","reserved_nanos":"0"})
}

#[tokio::test]
async fn preflight_failures_are_recorded_and_never_return_semantic_authority() {
    for (api, purpose, fault) in [
        ("typesafe", "goal", "cancelled"),
        ("openai", "intent", "expired"),
        ("perplexity", "goal", "profile"),
        ("cloudflare", "intent", "request"),
        ("typesafe", "goal", "endpoint"),
    ] {
        assert_preflight(api, purpose, fault).await;
    }
}

async fn assert_preflight(api: &'static str, purpose: &str, fault: &'static str) {
    let reservation = failed_dispatch(api, purpose, fault);
    let (plane, mut calls) = plane(move |call| preflight_reply(call, &reservation, fault));
    let host = Host::new();
    if fault == "cancelled" {
        host.stop.cancel();
    }
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING, &host.spool, &host.held, &host.stop);
    let result = verbs.task_decision(input()).await.unwrap();
    assert_eq!(result.state, DecisionCallState::NotSent);
    assert!(result.recorded.is_none());
    let requests = drain(&mut calls);
    assert_eq!(
        requests.iter().map(|call| call.verb).collect::<Vec<_>>(),
        vec![
            Verb::TaskDecisionReserve,
            Verb::Activity,
            Verb::TaskDecisionRecord,
            Verb::Activity
        ]
    );
    let progress: Vec<serde_json::Value> = requests
        .iter()
        .filter(|call| call.verb == Verb::Activity)
        .map(|call| serde_json::from_slice(call.body.as_ref().unwrap()).unwrap())
        .collect();
    assert_eq!(
        *progress
            .first()
            .unwrap()
            .pointer("/frames/0/task_decision_started/provider")
            .unwrap(),
        api
    );
    assert_eq!(
        *progress
            .first()
            .unwrap()
            .pointer("/frames/0/task_decision_started/operation")
            .unwrap(),
        purpose
    );
    assert_eq!(
        *progress
            .get(1)
            .unwrap()
            .pointer("/frames/0/task_decision_completed/phase")
            .unwrap(),
        "unavailable"
    );
    assert!(!host.spool.drain_decisions(&plane).await.unwrap());
    assert_eq!(drain(&mut calls).len(), 0);
}

fn preflight_reply(
    call: &crate::client::Call,
    reservation: &serde_json::Value,
    _fault: &str,
) -> Answer {
    match call.verb {
        Verb::TaskDecisionReserve => {
            let sent: DecisionReservationInput =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            assert_eq!(sent.fencing_token, FENCING);
            assert!(sent.retry_of.is_none());
            json(reservation)
        }
        Verb::TaskDecisionRecord => {
            let receipt: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            assert_eq!(*receipt.get("fencing_token").unwrap(), FENCING);
            assert_eq!(*receipt.pointer("/receipt/outcome").unwrap(), "not_sent");
            assert_eq!(*receipt.pointer("/receipt/cause").unwrap(), "preflight");
            json(&acknowledged())
        }
        Verb::Activity => json(&serde_json::json!({})),
        _ => Answer::Fail(crate::error::config("unexpected decision request")),
    }
}

#[tokio::test]
async fn unacknowledged_receipt_survives_and_duplicate_dispatch_cannot_send_again() {
    let (plane, mut calls) = plane(|call| match call.verb {
        Verb::TaskDecisionReserve => json(&dispatch("typesafe", "goal")),
        Verb::TaskDecisionRecord => Answer::Fail(crate::error::unavailable(call.verb, 503)),
        Verb::Activity => json(&serde_json::json!({})),
        _ => Answer::Fail(crate::error::config("unexpected decision request")),
    });
    let host = Host::new();
    host.stop.cancel();
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING, &host.spool, &host.held, &host.stop);
    assert!(matches!(
        verbs.task_decision(input()).await,
        Err(Unanswered::Unreachable)
    ));
    drain(&mut calls);
    assert!(matches!(
        verbs.task_decision(input()).await,
        Err(Unanswered::Unreachable)
    ));
    assert_eq!(
        drain(&mut calls)
            .iter()
            .map(|call| call.verb)
            .collect::<Vec<_>>(),
        vec![Verb::TaskDecisionReserve]
    );
    let (recovered, mut receipts) = crate::test_support::plane(|call| {
        assert_eq!(call.verb, Verb::TaskDecisionRecord);
        let body: serde_json::Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
        assert_eq!(*body.pointer("/receipt/outcome").unwrap(), "not_sent");
        json(&acknowledged())
    });
    // A nonempty page requests another sweep even when every receipt was acknowledged.
    assert!(host.spool.drain_decisions(&recovered).await.unwrap());
    assert_eq!(drain(&mut receipts).len(), 1);
    // The next sweep clears the saved cursor without replaying the acknowledged receipt.
    assert!(host.spool.drain_decisions(&recovered).await.unwrap());
    assert_eq!(drain(&mut receipts).len(), 0);
    assert!(!host.spool.drain_decisions(&recovered).await.unwrap());
    assert_eq!(drain(&mut receipts).len(), 0);
}

#[tokio::test]
async fn mismatched_dispatch_identity_never_creates_a_receipt_or_progress() {
    let (plane, mut calls) = plane(|call| {
        assert_eq!(call.verb, Verb::TaskDecisionReserve);
        let mut value = dispatch("typesafe", "goal");
        *value.pointer_mut("/dispatch/request/call_id").unwrap() = serde_json::json!(LEASE_ID);
        json(&value)
    });
    let host = Host::new();
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING, &host.spool, &host.held, &host.stop);
    assert!(matches!(
        verbs.task_decision(input()).await,
        Err(Unanswered::Unreachable)
    ));
    assert_eq!(drain(&mut calls).len(), 1);
    assert!(!host.spool.drain_decisions(&plane).await.unwrap());
    assert_eq!(drain(&mut calls).len(), 0);
}

fn failed_dispatch(api: &str, purpose: &str, fault: &str) -> serde_json::Value {
    let mut reservation = dispatch(api, purpose);
    match fault {
        "expired" => {
            *reservation.pointer_mut("/dispatch/expires_at").unwrap() =
                serde_json::json!(afd_core::clock::now());
        }
        "profile" => {
            *reservation
                .pointer_mut("/dispatch/assignment/profile/revision")
                .unwrap() = serde_json::json!("");
        }
        "request" => {
            *reservation
                .pointer_mut("/dispatch/request/questions")
                .unwrap() = serde_json::json!([]);
        }
        "endpoint" => {
            *reservation
                .pointer_mut("/dispatch/assignment/profile/endpoint")
                .unwrap() = serde_json::json!("http://decision.invalid");
        }
        _ => {}
    }
    reservation
}
