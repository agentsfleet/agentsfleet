//! A separate accepted source completes only after every item has an immutable answer.

use af_core::{error_code, id::Uuid7};
use af_wire::{
    event::EventType,
    task::{DecisionTaskVersion, TaskInputMode, TaskInputReceiptInput, TaskLimits},
};
use afd_admission::{Admission, Key, Producer, Reply};
use afd_connector::Provider;
use afd_fleet::task::{TaskBinding, TaskLifecycle as _};
use serde_json::{Value, json};

use super::fixture::{Case, GOALS, exercise, id};

const THREAD: &str =
    r#"{"team_id":"T024BE7LD","channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
const ANSWER: &str =
    "First answer: café\n\nQueued: Prevent duplicate bookmarks in PR 102\n\nThird answer: repaired";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn separate_source_answers_complete_in_original_order_without_a_destination() {
    exercise(async |case| scenario(case, Reply::None).await).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn separate_source_answers_owe_one_reply_to_the_original_destination() {
    exercise(async |case| {
        scenario(
            case,
            Reply::To {
                connector: Provider::Slack.id(),
                address: THREAD,
            },
        )
        .await;
    })
    .await;
}

async fn scenario(case: &Case, reply: Reply<'_>) {
    let source = source(case, reply).await;
    let plan = case.source_plan(source.clone(), &"a".repeat(64), 3, TaskLimits::default());
    let bindings = case
        .tasks
        .bind(case.lease(), &plan)
        .await
        .expect("accepted separate source");
    assert_eq!(bindings.items.len(), 3);
    interpreted(case, &source).await;
    let before_lease = lease(case).await;
    let mut accepted = Vec::new();
    for (index, item) in bindings.items.iter().enumerate().rev() {
        let input = receipt(case, &source, item, index);
        let result = case
            .tasks
            .accept_input(case.lease(), &input)
            .await
            .expect("item answer");
        assert!(!result.replayed);
        assert_eq!(result.item_id, item.item_id.to_string());
        assert_eq!(result.mode, input.mode);
        assert_eq!(result.covered_input_sequence, Some(0));
        accepted.push(input);
        if index != 0 {
            assert_pending(case, &source).await;
        }
    }
    let saved = assert_completed(case, &source, reply).await;
    replay(case, &accepted).await;
    refuse_replacement(case, accepted.first().expect("three accepted receipts")).await;
    assert_eq!(assert_completed(case, &source, reply).await, saved);
    assert_eq!(
        lease(case).await,
        before_lease,
        "answering input does not settle its owning execution"
    );
}

async fn source(case: &Case, reply: Reply<'_>) -> Uuid7 {
    let key = afd_db::test_util::mint_id();
    let actor = afd_events::steer_actor("native_task_owner");
    let request = json!({"message": GOALS.join("; ")}).to_string();
    let admitted = case
        .admissions
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Repeated(&key),
            fleet: case.scope.fleet_id.as_str(),
            workspace: case.scope.workspace_id.as_str(),
            actor: &actor,
            event_type: EventType::Chat,
            request_json: &request,
            reply,
        })
        .await
        .expect("separate human source with original reply destination");
    let source = id(&admitted.stored.id);
    assert_ne!(source.as_str(), case.held.event_id);
    case.claim(&source).await;
    source
}

fn receipt(
    case: &Case,
    source: &Uuid7,
    item: &TaskBinding,
    index: usize,
) -> garde::Valid<TaskInputReceiptInput> {
    let (mode, response) = match index {
        0 => (
            TaskInputMode::Answered,
            Some("First answer: café".to_owned()),
        ),
        1 => (TaskInputMode::Queued, None),
        _ => (
            TaskInputMode::Answered,
            Some("Third answer: repaired".to_owned()),
        ),
    };
    garde::Unvalidated::new(TaskInputReceiptInput {
        fencing_token: case.lease().fencing_token,
        operation_id: afd_db::test_util::mint_id(),
        source_admission_id: source.to_string(),
        plan_digest: "a".repeat(64),
        item_id: item.item_id.to_string(),
        task: Some(DecisionTaskVersion {
            task_id: item.task_id.as_ref().expect("accepted goal").to_string(),
            generation: 1,
        }),
        mode,
        response_text: response,
    })
    .validate()
    .expect("bounded answer receipt")
}

async fn assert_pending(case: &Case, source: &Uuid7) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("completion observer");
    let state: (bool, i64, i64) = sqlx::query_as(
        "SELECT settled_at IS NOT NULL, (SELECT count(*) FROM fleet.events WHERE fleet_id=$2::uuid AND event_id=$1), (SELECT count(*) FROM fleet.obligations WHERE fleet_id=$2::uuid AND event_id=$1) FROM fleet.admissions WHERE id=$1::uuid"
    ).bind(source.as_str()).bind(case.scope.fleet_id.as_str()).fetch_one(&mut *connection).await.expect("incomplete source");
    assert_eq!(
        state,
        (false, 0, 0),
        "one missing answer prevents completion and delivery"
    );
}

async fn assert_completed(case: &Case, source: &Uuid7, reply: Reply<'_>) -> Value {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("completed source observer");
    let events: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(e) FROM fleet.events e WHERE fleet_id=$1::uuid AND event_id=$2",
    )
    .bind(case.scope.fleet_id.as_str())
    .bind(source.as_str())
    .fetch_all(&mut *connection)
    .await
    .expect("original event");
    assert_eq!(events.len(), 1);
    let event = events.first().expect("one completed event");
    assert_event(event);
    let settled: bool = sqlx::query_scalar("SELECT settled_at IS NOT NULL AND parked_at IS NULL FROM fleet.admissions WHERE id=$1::uuid")
        .bind(source.as_str()).fetch_one(&mut *connection).await.expect("settled original source");
    assert!(settled);
    let obligations: Vec<(String, Option<String>, String)> = sqlx::query_as("SELECT provider,destination,answer FROM fleet.obligations WHERE fleet_id=$1::uuid AND event_id=$2")
        .bind(case.scope.fleet_id.as_str()).bind(source.as_str()).fetch_all(&mut *connection).await.expect("original reply obligations");
    let expected = match reply {
        Reply::To { connector, address } => vec![(
            connector.to_owned(),
            Some(address.to_owned()),
            ANSWER.to_owned(),
        )],
        _ => Vec::new(),
    };
    assert_eq!(obligations, expected);
    event.clone()
}

async fn lease(case: &Case) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(l) FROM runner.leases l WHERE id=$1::uuid")
        .bind(case.held.lease_key.as_str())
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("owning lease observer"),
        )
        .await
        .expect("original execution lease")
}

async fn refuse_replacement(case: &Case, original: &TaskInputReceiptInput) {
    let mut replacement = original.clone();
    replacement.operation_id = afd_db::test_util::mint_id();
    replacement.response_text =
        Some("Replacement must not reach the original conversation".to_owned());
    let replacement = garde::Unvalidated::new(replacement)
        .validate()
        .expect("bounded conflicting answer");
    let error = case
        .tasks
        .accept_input(case.lease(), &replacement)
        .await
        .expect_err("first answer is immutable");
    assert_eq!(error.code(), error_code::TASK_REPLAY_CONFLICT);
}

async fn replay(case: &Case, accepted: &[garde::Valid<TaskInputReceiptInput>]) {
    for input in accepted {
        let result = case
            .tasks
            .accept_input(case.lease(), input)
            .await
            .expect("exact receipt replay");
        assert!(result.replayed);
        assert_eq!(result.item_id, input.item_id);
        assert_eq!(result.mode, input.mode);
        assert_eq!(result.covered_input_sequence, Some(0));
    }
}

fn assert_event(event: &Value) {
    assert_eq!(
        event.get("actor"),
        Some(&json!(afd_events::steer_actor("native_task_owner")))
    );
    assert_eq!(
        event.get("request_json"),
        Some(&json!({"message": GOALS.join("; ")}))
    );
    assert_eq!(
        event.get("event_type"),
        Some(&json!(EventType::Chat.as_str()))
    );
    assert_eq!(
        event.get("status"),
        Some(&json!(af_core::event::status::PROCESSED))
    );
    assert_eq!(event.get("response_text"), Some(&json!(ANSWER)));
    assert_eq!(event.get("token_count"), Some(&json!(0)));
    assert_eq!(event.get("wall_ms"), Some(&json!(0)));
}

async fn interpreted(case: &Case, source: &Uuid7) {
    // PreparedPlan::from_parts supplies accepted meaning without a paid model call.
    // Its original source must also carry the persisted interpretation boundary.
    let updated = sqlx::query("UPDATE fleet.admissions SET interpreted_at=clock_timestamp() WHERE id=$1::uuid AND input_claim_lease_id=$2::uuid AND interpreted_at IS NULL")
        .bind(source.as_str()).bind(case.held.lease_key.as_str())
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("accepted interpretation fixture"))
        .await.expect("accepted source interpretation boundary");
    assert_eq!(updated.rows_affected(), 1);
}
