//! A task run's publications: the list and a read by id, a publication an
//! API-only task has no authority for, and an operation that already holds a
//! receipt answered from it.
//!
//! A receipt is seeded as the native allocation writes it, because a fresh
//! publication needs the GitHub boundary this suite does not run; the replay
//! and read paths never reach it.

use af_core::{etag::sha256_hex, id::Uuid7};
use af_wire::durable_task::{DecisionDurableTaskVersion, DurableTaskFileChange};
use afd_db::{config::DbRole, test_util::mint_id};

use super::binding::{Bound, bind};
use super::*;

/// The head the publication was prepared against, the tree the run tested and
/// the commit a confirmed receipt names.
const HEAD: &str = "0000000000000000000000000000000000000000";
const TREE: &str = "1111111111111111111111111111111111111111";
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

/// One publication of a README change for `bound`'s task under `operation`.
fn publication(fixture: &Fixture, bound: &Bound, operation: &Uuid7) -> Value {
    json!({"operation_id": operation, "fencing_token": fixture.run.fence,
        "durable_task": bound.version(), "source_item_id": bound.item,
        "covered_input_sequence": 0, "expected_head": HEAD, "tested_tree": TREE,
        "changes": [{"path": "README.md", "contents": "aGk="}]})
}

fn digest(bound: &Bound) -> String {
    let task: DecisionDurableTaskVersion =
        serde_json::from_value(bound.version()).expect("the bound task version");
    let changes = [DurableTaskFileChange {
        path: "README.md".to_owned(),
        contents: Some("aGk=".to_owned()),
    }];
    let canonical = serde_json::to_string(&Canonical {
        task: &task,
        source_item_id: &bound.item,
        covered_input_sequence: 0,
        expected_head: HEAD,
        tested_tree: TREE,
        changes: &changes,
    })
    .expect("a canonical publication");
    sha256_hex(canonical.as_bytes())
}

const SEED_EFFECT: &str = "\
INSERT INTO fleet.durable_task_effects (id, durable_task_id, fleet_id, workspace_id,
    source_item_id, original_run_id, original_runner_id, original_fence,
    durable_task_generation, stop_generation, ordinal, operation_id,
    covered_input_sequence, expected_head, tested_tree, request_digest,
    issuance_expires_at, github_repository_id, github_installation_id, status,
    commit_oid, target, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, 0, $9, $10, 0, $11, $12, $13, now(), 41, 42, $14,
    $15, $16, now(), now())";

/// Seeds the receipt the native allocation leaves for `operation`, in `state`.
async fn seed(
    fixture: &Fixture,
    bound: &Bound,
    operation: &Uuid7,
    (ordinal, state, commit): (i64, &str, Option<&str>),
) -> Uuid7 {
    let effect = mint_id();
    let target = json!({"repository": "example/linkwarden", "repository_id": 41,
        "installation_id": 42, "number": 7, "branch": "repair", "base": "main",
        "head": HEAD, "tree": TREE, "url": PULL_REQUEST, "workflows": []});
    let fence = i64::try_from(fixture.run.fence).expect("the fence fits");
    sqlx::query(SEED_EFFECT)
        .bind(&effect)
        .bind(Uuid7::parse(&bound.task).expect("a task id"))
        .bind(&fixture.owner.fleet)
        .bind(&fixture.owner.workspace)
        .bind(Uuid7::parse(&bound.item).expect("an item id"))
        .bind(&fixture.run.id)
        .bind(&fixture.runner.id)
        .bind(fence)
        .bind(ordinal)
        .bind(operation)
        .bind(HEAD)
        .bind(TREE)
        .bind(digest(bound))
        .bind(state)
        .bind(commit)
        .bind(target)
        .execute(&mut *fixture.database.acquire().await.expect("a connection"))
        .await
        .expect("the publication intent, its state and its private target");
    effect
}

/// Removes the seeded receipts, which carry no foreign key the tenant's
/// cleanup could follow.
async fn unseed(effects: &[Uuid7]) {
    let lane = afd_db::test_util::TestDatabase::shared();
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let mut connection = owner.acquire().await.expect("a cleanup connection");
    for effect in effects {
        sqlx::query("DELETE FROM fleet.durable_task_effects WHERE id = $1")
            .bind(effect)
            .execute(&mut *connection)
            .await
            .expect("the seeded receipt goes");
    }
    drop(connection);
    owner.close().await;
}

// Value: protects=`GET …/effects` lists a task's publications under the run's
// fence, `GET …/effects/{id}` refuses an id the task never published, and
// `POST …/effects` refuses an API-only task's publication before allocating
// one; fails_when=a malformed query is read, a missing id answers, or a
// publication without GitHub authority is allocated; seam=the production
// router over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_api_only_tasks_publication_is_refused_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let route = path(&fixture, "effects");
    let list = format!(
        "{}&durable_task_id={}",
        fenced(&fixture, &route),
        bound.task
    );
    let empty = read(&fixture, &list, StatusCode::OK).await;
    assert_eq!(empty, json!({"effects": [], "next_cursor": null}));
    let malformed = format!("{route}?fencing_token=not-a-number");
    let refused = read(&fixture, &malformed, StatusCode::BAD_REQUEST).await;
    problem(&refused, "UZ-REQ-001");
    let missing = fenced(&fixture, &format!("{route}/{}", mint_id()));
    let refused = read(&fixture, &missing, StatusCode::NOT_FOUND).await;
    problem(&refused, "UZ-DT-001");

    let body = publication(&fixture, &bound, &mint_id());
    let refused = call(&fixture, Method::POST, &route, &body, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-DT-004");
    let unchanged = read(&fixture, &list, StatusCode::OK).await;
    assert_eq!(
        unchanged, empty,
        "a refused target allocates no publication"
    );
    fixture.finish().await;
}

// Value: protects=a publication whose operation already holds a confirmed
// receipt replays it as a 200 and reads back by id, and one holding a refused
// receipt answers a coded 409 carrying it; fails_when=a replay re-publishes,
// or the read or the conflict loses the receipt; seam=the production router
// over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_saved_publication_replays_its_receipt_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let route = path(&fixture, "effects");
    let confirmed_operation = mint_id();
    let state = (1, "confirmed", Some(COMMIT));
    let confirmed = seed(&fixture, &bound, &confirmed_operation, state).await;
    let body = publication(&fixture, &bound, &confirmed_operation);
    let replayed = call(&fixture, Method::POST, &route, &body, StatusCode::OK).await;
    let receipt = json!({"effect_id": confirmed, "ordinal": 1, "state": "confirmed",
        "expected_head": HEAD, "tested_tree": TREE, "commit": COMMIT,
        "pull_request_url": PULL_REQUEST});
    assert_eq!(replayed, receipt, "a confirmed operation replays as a 200");
    let one = fenced(&fixture, &format!("{route}/{confirmed}"));
    let read_back = read(&fixture, &one, StatusCode::OK).await;
    assert_eq!(read_back, receipt, "a read by id returns the same receipt");

    let refused_operation = mint_id();
    let refused = seed(&fixture, &bound, &refused_operation, (2, "refused", None)).await;
    let body = publication(&fixture, &bound, &refused_operation);
    let conflict = call(&fixture, Method::POST, &route, &body, StatusCode::CONFLICT).await;
    problem(&conflict, "UZ-DT-002");
    assert_eq!(
        field(&conflict, "/effect"),
        &json!({"effect_id": refused, "ordinal": 2, "state": "refused",
            "expected_head": HEAD, "tested_tree": TREE, "commit": null,
            "pull_request_url": PULL_REQUEST}),
        "the conflict carries the refused receipt"
    );
    unseed(&[confirmed, refused]).await;
    fixture.finish().await;
}
