//! Paid current intent proof precedes complete native binding.
use super::*;
pub(super) async fn bind(fixture: &Fixture) -> (TaskBindingsInput, Value) {
    let input_path = format!(
        "{}?fencing_token={}",
        path(fixture, "inputs"),
        fixture.lease.fence
    );
    let before = success(fixture, Method::GET, &input_path, "", StatusCode::OK).await;
    assert_eq!(before["current_message"], setup::MESSAGE);
    let issued = call(
        fixture,
        Method::POST,
        &fixture.collection(),
        &serde_json::to_value(fixture.reservation()).expect("reservation"),
        StatusCode::CREATED,
    )
    .await;
    let request = assertions::dispatch(fixture, &issued);
    let result = new_work_answer(&request);
    let receipt = DecisionReceiptInput {
        fencing_token: fixture.lease.fence,
        receipt: DecisionReceipt::Provider { result },
    };
    call(
        fixture,
        Method::PATCH,
        &fixture.item(&request.call_id),
        &serde_json::to_value(receipt).expect("decision receipt"),
        StatusCode::OK,
    )
    .await;
    let binding = binding_input(fixture, &before, request.call_id);
    let body = serde_json::to_value(&binding).expect("binding");
    let route = path(fixture, "bindings");
    foreign_and_stale(fixture, &route, &body).await;
    let accepted = call(fixture, Method::POST, &route, &body, StatusCode::CREATED).await;
    assert_eq!(accepted["replayed"], false);
    assert!(accepted["current_task"].is_object());
    let replay = call(fixture, Method::POST, &route, &body, StatusCode::OK).await;
    assert_eq!(replay["replayed"], true);
    assert_eq!(accepted["items"], replay["items"]);
    (binding, accepted)
}

fn new_work_answer(
    request: &afd_wire::decision::DecisionRequest,
) -> afd_wire::decision::DecisionResult {
    let mut result = crate::decision_reply::known_result(request);
    let DecisionOutcome::Answered { answers } = &mut result.result else {
        panic!("synthetic successful provider answers");
    };
    for (name, answer) in answers {
        match answer {
            DecisionAnswer::Noul { noul } => *noul = 0.0,
            DecisionAnswer::Choice {
                choice,
                probabilities,
                ..
            } => {
                assert!(matches!(name.as_str(), "intent" | "association"));
                *choice = "new_work".to_owned();
                assert!(probabilities.contains_key("new_work"));
                for (option, probability) in probabilities {
                    *probability = if option == "new_work" { 1.0 } else { 0.0 };
                }
            }
            DecisionAnswer::Score { .. } => panic!("message questions have no score"),
        }
    }
    result
}

fn binding_input(fixture: &Fixture, before: &Value, call_id: String) -> TaskBindingsInput {
    let mut binding = TaskBindingsInput {
        fencing_token: fixture.lease.fence,
        source_admission_id: fixture.lease.source.clone(),
        accepted_revision_id: before["accepted_revision_id"]
            .as_str()
            .expect("accepted setup")
            .to_owned(),
        plan_digest: String::new(),
        message_decision_call_id: call_id,
        items: vec![TaskBindingItem {
            decision_call_id: None,
            proposed: TaskProposedItem {
                source_start: 0,
                source_end: i32::try_from(setup::MESSAGE.len()).expect("small message"),
                action: TaskInputAction::NewWork,
                task_id: None,
                title: "Investigate checks".to_owned(),
                interpretation: "Read and explain the failed checks".to_owned(),
                target: Some(TaskRequestedTarget::ApiOnly),
            },
        }],
    };
    let digest = sha256_hex(&serde_json::to_vec(&binding.interpretation()).expect("meaning"));
    binding.plan_digest = digest;
    binding
}
