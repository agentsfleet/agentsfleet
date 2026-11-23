//! A publication's operation that already holds a saved receipt is answered
//! from that receipt: a confirmed one replays as a 200, any other state as a
//! coded 409 carrying the receipt, and a read by id returns it whole.
//!
//! The receipt is seeded as the native allocation writes it, because a fresh
//! publication needs the GitHub boundary this suite does not run; the replay
//! and read paths never reach it.

use af_core::id::Uuid7;
use af_wire::durable_task::{DecisionDurableTaskVersion, DurableTaskFileChange};
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;

use super::*;

/// The head the seeded receipt was prepared against.
const HEAD: &str = "0000000000000000000000000000000000000000";
/// The tree the runner tested.
const TREE: &str = "1111111111111111111111111111111111111111";
/// The commit a confirmed receipt names.
const COMMIT: &str = "3333333333333333333333333333333333333333";
/// The pull request the receipt's target names.
const PULL_REQUEST: &str = "https://github.com/example/linkwarden/pull/7";

/// The request digest the native allocation keys an operation by: the same
/// fields, in the same order, as `afd_fleet`'s effect allocation hashes.
#[derive(serde::Serialize)]
struct Canonical<'a> {
    task: &'a DecisionDurableTaskVersion,
    source_item_id: &'a str,
    covered_input_sequence: u64,
    expected_head: &'a str,
    tested_tree: &'a str,
    changes: &'a [DurableTaskFileChange],
}

/// One publication request for `task`, sourced from `item`.
fn publication(fixture: &Fixture, task: &Value, item: &Value, operation: &Uuid7) -> Value {
    json!({"operation_id":operation,"fencing_token":fixture.run.fence,
        "durable_task":task,"source_item_id":item,"covered_input_sequence":0,
        "expected_head":HEAD,"tested_tree":TREE,
        "changes":[{"path":"README.md","contents":"aGk="}]})
}

fn digest(task: &Value, item: &str) -> String {
    let task: DecisionDurableTaskVersion =
        serde_json::from_value(task.clone()).expect("bound task version");
    let changes = [DurableTaskFileChange {
        path: "README.md".to_owned(),
        contents: Some("aGk=".to_owned()),
    }];
    let canonical = serde_json::to_string(&Canonical {
        task: &task,
        source_item_id: item,
        covered_input_sequence: 0,
        expected_head: HEAD,
        tested_tree: TREE,
        changes: &changes,
    })
    .expect("canonical publication");
    sha256_hex(canonical.as_bytes())
}

/// Seeds the receipt the native allocation leaves for `operation`, in `state`.
async fn seed(
    fixture: &Fixture,
    task: &Value,
    item: &str,
    operation: &Uuid7,
    (ordinal, state, commit): (i64, &str, Option<&str>),
) -> Uuid7 {
    let task_id = Uuid7::parse(
        task.pointer("/durable_task_id")
            .and_then(Value::as_str)
            .expect("bound task id"),
    )
    .expect("task id is a UUIDv7");
    let generation = task
        .pointer("/generation")
        .and_then(Value::as_i64)
        .expect("bound generation");
    let effect = mint_id();
    let mut connection = fixture.database.acquire().await.expect("seed connection");
    let (fleet, workspace): (Uuid7, Uuid7) =
        sqlx::query_as("SELECT fleet_id, workspace_id FROM fleet.durable_tasks WHERE id = $1")
            .bind(&task_id)
            .fetch_one(&mut *connection)
            .await
            .expect("the bound task's scope");
    let target = json!({"repository":"example/linkwarden","repository_id":41,
        "installation_id":42,"number":7,"branch":"repair","base":"main",
        "head":HEAD,"tree":TREE,"url":PULL_REQUEST,"workflows":[]});
    sqlx::query(
        "INSERT INTO fleet.durable_task_effects (id, durable_task_id, fleet_id, workspace_id,
            source_item_id, original_run_id, original_runner_id, original_fence,
            durable_task_generation, stop_generation, ordinal, operation_id,
            covered_input_sequence, expected_head, tested_tree, request_digest,
            issuance_expires_at, github_repository_id, github_installation_id, status,
            commit_oid, target, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,0,$10,$11,0,$12,$13,$14,now(),41,42,$15,$16,$17,
            now(),now())",
    )
    .bind(&effect)
    .bind(&task_id)
    .bind(&fleet)
    .bind(&workspace)
    .bind(Uuid7::parse(item).expect("item id is a UUIDv7"))
    .bind(&fixture.run.id)
    .bind(&fixture.runner.id)
    .bind(i64::try_from(fixture.run.fence).expect("fence fits"))
    .bind(generation)
    .bind(ordinal)
    .bind(operation)
    .bind(HEAD)
    .bind(TREE)
    .bind(digest(task, item))
    .bind(state)
    .bind(commit)
    .bind(target)
    .execute(&mut *connection)
    .await
    .expect("seed the publication intent, its state and its private target");
    effect
}

/// Removes the seeded receipts; the tenant's own cleanup cannot reach the
/// append-only intent rows, which carry no foreign key.
async fn unseed(effects: &[Uuid7]) {
    let lane = TestDatabase::shared();
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let mut connection = owner.acquire().await.expect("cleanup connection");
    for effect in effects {
        sqlx::query("DELETE FROM fleet.durable_task_effects WHERE id = $1")
            .bind(effect)
            .execute(&mut *connection)
            .await
            .expect("remove the seeded receipt");
    }
    drop(connection);
    owner.close().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_saved_publication_replays_its_receipt_and_reads_back_by_id() {
    let fixture = Fixture::create().await;
    let (_, accepted) = binding::bind(&fixture).await;
    let task = json!({"durable_task_id":accepted.pointer("/current_durable_task/durable_task_id").expect("response field exists"),
        "generation":accepted.pointer("/current_durable_task/generation").expect("response field exists")});
    let item = accepted
        .pointer("/items/0/item_id")
        .and_then(Value::as_str)
        .expect("accepted item")
        .to_owned();
    let route = path(&fixture, "effects");

    let confirmed_operation = mint_id();
    let confirmed = seed(
        &fixture,
        &task,
        &item,
        &confirmed_operation,
        (1, "confirmed", Some(COMMIT)),
    )
    .await;
    let replayed = call(
        &fixture,
        Method::POST,
        &route,
        &publication(&fixture, &task, &json!(item), &confirmed_operation),
        StatusCode::OK,
    )
    .await;
    let receipt = json!({"effect_id":confirmed,"ordinal":1,"state":"confirmed",
        "expected_head":HEAD,"tested_tree":TREE,"commit":COMMIT,"pull_request_url":PULL_REQUEST});
    assert_eq!(replayed, receipt, "a confirmed operation replays as a 200");
    let read = success(
        &fixture,
        Method::GET,
        &format!("{route}/{confirmed}?fencing_token={}", fixture.run.fence),
        "",
        StatusCode::OK,
    )
    .await;
    assert_eq!(read, receipt, "a read by id returns the same receipt");

    let refused_operation = mint_id();
    let refused = seed(
        &fixture,
        &task,
        &item,
        &refused_operation,
        (2, "refused", None),
    )
    .await;
    let response = send(
        &fixture.router,
        Method::POST,
        &route,
        Some(&fixture.runner.token),
        &publication(&fixture, &task, &json!(item), &refused_operation).to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response
            .headers()
            .get(http::header::CACHE_CONTROL)
            .expect("a conflict carrying a receipt prohibits caching"),
        "no-store"
    );
    let conflict = json_body(response).await;
    problem(&conflict, "UZ-DT-002");
    assert_eq!(
        conflict.pointer("/effect"),
        Some(&json!({"effect_id":refused,"ordinal":2,"state":"refused",
            "expected_head":HEAD,"tested_tree":TREE,"commit":null,"pull_request_url":PULL_REQUEST})),
        "the conflict carries the refused receipt"
    );

    unseed(&[confirmed, refused]).await;
    fixture.finish().await;
}
