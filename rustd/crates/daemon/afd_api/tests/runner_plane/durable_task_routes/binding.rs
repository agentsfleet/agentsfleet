//! A run binds one span of its person's message to a new task once, then
//! reads the inputs it holds and acknowledges the one it bound.

use af_core::{etag::sha256_hex, id::Uuid7};
use af_wire::decision::NoulCriteria;
use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskRequestedTarget,
    DurableTaskStatement,
};
use afd_db::test_util::mint_id;

use super::fixture::MESSAGE;
use super::*;

/// The task a run bound, and what its binding accepted.
pub(super) struct Bound {
    pub task: String,
    pub item: String,
    pub plan_digest: String,
}

impl Bound {
    /// The task at its first generation, as every task verb names it.
    pub(super) fn version(&self) -> Value {
        json!({"durable_task_id": self.task, "generation": 1})
    }
}

/// The `task_start` a run sends for the whole message, titled `title`, with
/// the digest the daemon computes for it.
fn task_start(fixture: &Fixture, title: &str) -> DurableTaskBindingsInput {
    let statement = DurableTaskStatement {
        id: "explained".to_owned(),
        question: "Are the pull request's failed checks explained?".to_owned(),
        criteria: NoulCriteria {
            yes: "Each failed check is named with its cause.".to_owned(),
            no: "A failed check is unexplained, or the evidence is missing.".to_owned(),
        },
    };
    let mut input = DurableTaskBindingsInput {
        fencing_token: fixture.run.fence,
        source_turn_id: fixture.run.source.to_string(),
        plan_digest: String::new(),
        items: vec![DurableTaskBindingItem::NewWork {
            source_start: 0,
            source_end: i32::try_from(MESSAGE.len()).expect("a short message"),
            title: title.to_owned(),
            target: DurableTaskRequestedTarget::ApiOnly,
            statements: vec![statement],
        }],
    };
    let digest = sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("a binding"));
    input.plan_digest = digest;
    input
}

/// Binds the whole message to a task over HTTP, and makes the fixture run the
/// task's run.
pub(super) async fn bind(fixture: &Fixture) -> Bound {
    let input = task_start(fixture, "Investigate checks");
    let body = serde_json::to_value(&input).expect("a binding body");
    let accepted = call(
        fixture,
        Method::POST,
        &path(fixture, "bindings"),
        &body,
        StatusCode::CREATED,
    )
    .await;
    let task = text(&accepted, "/items/0/durable_task_id").to_owned();
    let attached = fixture
        .tasks
        .attach_run(&fixture.run.id, &Uuid7::parse(&task).expect("a task id"), 1)
        .await
        .expect("the run attaches");
    assert!(attached, "a ready task takes its first run");
    Bound {
        item: text(&accepted, "/items/0/item_id").to_owned(),
        task,
        plan_digest: input.plan_digest,
    }
}

/// The fleet's tasks, by state.
async fn task_states(fixture: &Fixture) -> Vec<String> {
    sqlx::query_scalar("SELECT status FROM fleet.durable_tasks WHERE fleet_id = $1")
        .bind(&fixture.owner.fleet)
        .fetch_all(&mut *fixture.database.acquire().await.expect("a connection"))
        .await
        .expect("the fleet's tasks")
}

// Value: protects=`POST …/durable_tasks/bindings` writes one ready task from a
// one-item plan, answers its repeat from the stored binding, and refuses a
// stranger, a stale fence and a second plan for the same message;
// fails_when=the route skips the run's ownership or fence, a repeat allocates
// again, or one message takes two plans; seam=the production router over live
// Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_binds_once_over_http() {
    let fixture = Fixture::create().await;
    let route = path(&fixture, "bindings");
    let input = task_start(&fixture, "Investigate checks");
    let body = serde_json::to_value(&input).expect("a binding body");
    foreign_and_stale(&fixture, Method::POST, &route, &body).await;
    assert_eq!(task_states(&fixture).await, Vec::<String>::new());

    let accepted = call(&fixture, Method::POST, &route, &body, StatusCode::CREATED).await;
    assert_eq!(field(&accepted, "/replayed"), &json!(false));
    assert_eq!(field(&accepted, "/items/0/action"), &json!("new_work"));
    assert_eq!(
        field(&accepted, "/current_durable_task"),
        &Value::Null,
        "the run stays an ordinary run"
    );
    let replay = call(&fixture, Method::POST, &route, &body, StatusCode::OK).await;
    assert_eq!(field(&replay, "/replayed"), &json!(true));
    assert_eq!(field(&replay, "/items"), field(&accepted, "/items"));

    let other = task_start(&fixture, "A different plan");
    let other = serde_json::to_value(&other).expect("a binding body");
    let refused = call(&fixture, Method::POST, &route, &other, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-DT-003");
    assert_eq!(task_states(&fixture).await, ["ready"]);
    fixture.finish().await;
}

// Value: protects=`GET …/inputs` serves the bound run its person's message and
// the accepted plan under the run's own fence, and `POST …/input-receipts`
// records one answer for the bound item, replays it and refuses a different
// one; fails_when=the page ignores the fence, the receipt is written twice, or
// a changed answer overwrites the first; seam=the production router over live
// Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_inputs_page_and_its_receipt_answer_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let inputs = path(&fixture, "inputs");

    let page = read(&fixture, &fenced(&fixture, &inputs), StatusCode::OK).await;
    assert_eq!(field(&page, "/current_message"), &json!(MESSAGE));
    assert_eq!(
        field(&page, "/current_durable_task/durable_task_id"),
        &json!(bound.task)
    );
    assert_eq!(
        field(&page, "/current_interpretation/plan_digest"),
        &json!(bound.plan_digest)
    );
    let stale = format!("{inputs}?fencing_token={}", fixture.run.fence + 1);
    let refused = read(&fixture, &stale, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-RUN-005");
    assert_eq!(field(&refused, "/current_state"), &json!("superseded"));

    let receipt = json!({"fencing_token": fixture.run.fence, "operation_id": mint_id(),
        "source_turn_id": fixture.run.source, "plan_digest": bound.plan_digest,
        "item_id": bound.item, "durable_task": bound.version(), "mode": "answered",
        "response_text": "I will read the failed checks."});
    acknowledge(&fixture, &receipt).await;
    fixture.finish().await;
}

/// One receipt recorded, replayed, and a changed answer refused.
async fn acknowledge(fixture: &Fixture, receipt: &Value) {
    let route = path(fixture, "input-receipts");
    foreign_and_stale(fixture, Method::POST, &route, receipt).await;
    let first = call(fixture, Method::POST, &route, receipt, StatusCode::CREATED).await;
    assert_eq!(field(&first, "/replayed"), &json!(false));
    assert_eq!(field(&first, "/mode"), &json!("answered"));
    let replay = call(fixture, Method::POST, &route, receipt, StatusCode::OK).await;
    assert_eq!(field(&replay, "/replayed"), &json!(true));
    assert_eq!(
        field(&first, "/covered_input_sequence"),
        field(&replay, "/covered_input_sequence")
    );
    let mut changed = receipt.clone();
    *changed
        .get_mut("response_text")
        .expect("the receipt names its answer") = json!("A conflicting second answer");
    let refused = call(
        fixture,
        Method::POST,
        &route,
        &changed,
        StatusCode::CONFLICT,
    )
    .await;
    problem(&refused, "UZ-DT-003");
}
