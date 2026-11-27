//! A task run checkpoints its state: a plan, its writer pin, one page, the
//! commit, then a restore that reads the page back.

use af_core::etag::sha256_hex;
use af_crypto::checkpoint_digest;
use af_wire::durable_task::CheckpointPage;
use afd_db::test_util::mint_id;

use super::binding::{Bound, bind};
use super::*;

/// A one-page incremental plan for `bound`'s task, uploading up to `budget`
/// bytes.
pub(super) fn plan_input(fixture: &Fixture, bound: &Bound, budget: u64) -> Value {
    json!({"fencing_token": fixture.run.fence, "operation_id": mint_id(),
        "durable_task": bound.version(), "mode": "incremental", "base_id": null,
        "commit_oid": null, "format_version": 1, "toolbox_digest": "3".repeat(64),
        "page_count": 1, "upload_budget_bytes": budget, "logical_bytes": 0,
        "entry_count": 0})
}

/// The checkpoint `plan` allocated, as its member route.
pub(super) fn member(fixture: &Fixture, plan: &Value) -> String {
    format!(
        "{}/{}",
        path(fixture, "checkpoints"),
        text(plan, "/checkpoint_id")
    )
}

/// An empty first page and the digest it is written under.
fn empty_page() -> (CheckpointPage, String) {
    let contents = CheckpointPage {
        page_number: 0,
        entries: vec![],
        history_objects: vec![],
    };
    let digest = sha256_hex(&serde_json::to_vec(&contents).expect("a canonical page"));
    (contents, digest)
}

/// `body` refused under a fence one past the run's.
pub(super) async fn stale(fixture: &Fixture, method: Method, route: &str, body: &Value) {
    let mut stale = body.clone();
    *stale
        .get_mut("fencing_token")
        .expect("a fenced body names its fence") = json!(fixture.run.fence + 1);
    let refused = call(fixture, method, route, &stale, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-RUN-005");
}

// Value: protects=`POST …/checkpoints` allocates one plan per operation,
// `PATCH …/pins/{pin}` renews its writer pin once per version, `PUT
// …/pages/{page}` stores a page only under its digest, `PATCH
// …/checkpoints/{id}` commits once, and `POST …/authorizations` restores the
// committed root; fails_when=a repeat allocates again, a stale pin version
// renews, a corrupt page is stored, a commit or restore ignores the run's
// fence; seam=the production router over live Postgres and checkpoint custody.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_checkpoint_is_planned_committed_and_restored_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let collection = path(&fixture, "checkpoints");
    let input = plan_input(&fixture, &bound, 0);
    foreign_and_stale(&fixture, Method::POST, &collection, &input).await;

    let plan = call(
        &fixture,
        Method::POST,
        &collection,
        &input,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(field(&plan, "/replayed"), &json!(false));
    let replay = call(&fixture, Method::POST, &collection, &input, StatusCode::OK).await;
    assert_eq!(field(&replay, "/replayed"), &json!(true));
    assert_eq!(
        field(&replay, "/checkpoint_id"),
        field(&plan, "/checkpoint_id")
    );
    let route = member(&fixture, &plan);
    renew_pin(&fixture, &bound, &route, &plan).await;
    let (contents, digest) = empty_page();
    write_page(&fixture, &bound, &route, &plan, (&contents, &digest)).await;
    let commit = commit(&fixture, &bound, &route, &plan, &digest).await;
    restore(&fixture, &bound, &route, &commit).await;
    fixture.finish().await;
}

async fn renew_pin(fixture: &Fixture, bound: &Bound, route: &str, plan: &Value) {
    let pin = format!("{route}/pins/{}", text(plan, "/pin/id"));
    let renewal = json!({"fencing_token": fixture.run.fence, "durable_task": bound.version(),
        "resource_version": field(plan, "/pin/resource_version")});
    let renewed = call(fixture, Method::PATCH, &pin, &renewal, StatusCode::OK).await;
    let version = |value: &Value, pointer: &str| field(value, pointer).as_u64();
    assert!(version(&renewed, "/resource_version") > version(plan, "/pin/resource_version"));
    let refused = call(fixture, Method::PATCH, &pin, &renewal, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-DT-002");
}

async fn write_page(
    fixture: &Fixture,
    bound: &Bound,
    route: &str,
    plan: &Value,
    (contents, digest): (&CheckpointPage, &str),
) {
    let page = format!("{route}/pages/{}", text(plan, "/page_ids/0"));
    let write = json!({"fencing_token": fixture.run.fence, "durable_task": bound.version(),
        "pin_id": text(plan, "/pin/id"), "digest": digest, "contents": contents});
    for _ in 0..2 {
        let stored = call(fixture, Method::PUT, &page, &write, StatusCode::OK).await;
        assert_eq!(stored, serde_json::to_value(contents).expect("the page"));
    }
    let mut corrupt = write.clone();
    *corrupt.get_mut("digest").expect("the page's digest") = json!("f".repeat(64));
    let refused = call(fixture, Method::PUT, &page, &corrupt, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-DT-008");
}

async fn commit(
    fixture: &Fixture,
    bound: &Bound,
    route: &str,
    plan: &Value,
    digest: &str,
) -> Value {
    let page = text(plan, "/page_ids/0");
    let commit = json!({"fencing_token": fixture.run.fence, "durable_task": bound.version(),
        "pin_id": text(plan, "/pin/id"),
        "manifest_root": checkpoint_digest::manifest_root([(page, digest)]),
        "history_digest": checkpoint_digest::history_root([]),
        "covered_input_sequence": 0, "effect_watermark": 0, "uploaded": []});
    stale(fixture, Method::PATCH, route, &commit).await;
    let saved = call(fixture, Method::PATCH, route, &commit, StatusCode::OK).await;
    assert_eq!(
        field(&saved, "/manifest_root"),
        field(&commit, "/manifest_root")
    );
    assert_eq!(field(&saved, "/replayed"), &json!(false));
    let replay = call(fixture, Method::PATCH, route, &commit, StatusCode::OK).await;
    assert_eq!(field(&replay, "/replayed"), &json!(true));
    commit
}

/// A restore of the committed checkpoint names its root and no objects, and
/// a stale fence restores nothing.
async fn restore(fixture: &Fixture, bound: &Bound, route: &str, commit: &Value) {
    let authorizations = format!("{route}/authorizations");
    let restore = json!({"fencing_token": fixture.run.fence, "durable_task": bound.version(),
        "purpose": "restore", "pin_id": null, "object_ids": []});
    stale(fixture, Method::POST, &authorizations, &restore).await;
    let restored = call(
        fixture,
        Method::POST,
        &authorizations,
        &restore,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        field(&restored, "/plan/manifest_root"),
        field(commit, "/manifest_root")
    );
    assert_eq!(field(&restored, "/objects"), &json!([]));
    checkpoint_objects::read_back(fixture, route, &restored).await;
}
