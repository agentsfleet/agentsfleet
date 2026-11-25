//! A checkpoint's objects: registered once, uploaded only at the length their
//! plaintext implies, and read back through a restore pin.

use af_core::etag::sha256_hex;

use super::binding::bind;
use super::checkpoints::{member, plan_input, stale};
use super::*;

/// The page and objects a restore pin reads, and a read under another fence
/// or a malformed query refused.
pub(super) async fn read_back(fixture: &Fixture, route: &str, restored: &Value) {
    let reader = text(restored, "/plan/pin/id");
    let page = format!("{route}/pages/{}", text(restored, "/plan/page_ids/0"));
    let query = |fence: u64| format!("fencing_token={fence}&pin_id={reader}");
    let read_page = read(
        fixture,
        &format!("{page}?{}", query(fixture.run.fence)),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        read_page,
        json!({"page_number": 0, "entries": [], "history_objects": []})
    );
    let stale_page = format!("{page}?{}", query(fixture.run.fence + 1));
    let refused = read(fixture, &stale_page, StatusCode::CONFLICT).await;
    problem(&refused, "UZ-RUN-005");

    let objects = format!("{route}/objects");
    let listed = read(
        fixture,
        &format!("{objects}?{}", query(fixture.run.fence)),
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed, json!({"items": [], "next_cursor": null}));
    let malformed = format!("{objects}?fencing_token=not-a-number&pin_id={reader}");
    let refused = read(fixture, &malformed, StatusCode::BAD_REQUEST).await;
    problem(&refused, "UZ-REQ-001");
}

// Value: protects=`POST …/objects` registers a checkpoint's object once and
// answers its repeat with the same key, and `POST …/uploads` takes only the
// ciphertext length one plaintext byte implies to custody; fails_when=a repeat
// mints a second object or nonce, a stale fence registers, or a wrong length
// reaches the store; seam=the production router over live Postgres and
// checkpoint custody; no_claim=a signed upload, which needs an object store
// that can prove custody, proven by `afd_fleet`'s checkpoint custody suite.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checkpoint_objects_register_once_and_upload_at_their_length_over_http() {
    let fixture = Fixture::create().await;
    let bound = bind(&fixture).await;
    let input = plan_input(&fixture, &bound, 64);
    let collection = path(&fixture, "checkpoints");
    let plan = call(
        &fixture,
        Method::POST,
        &collection,
        &input,
        StatusCode::CREATED,
    )
    .await;
    let route = member(&fixture, &plan);
    let objects = format!("{route}/objects");
    let registration = json!({"fencing_token": fixture.run.fence,
        "durable_task": bound.version(), "pin_id": text(&plan, "/pin/id"),
        "objects": [{"sha256": sha256_hex(b"x"), "size": 1, "kind": "file"}]});
    stale(&fixture, Method::POST, &objects, &registration).await;

    let registered = call(
        &fixture,
        Method::POST,
        &objects,
        &registration,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(field(&registered, "/items/0/status"), &json!("missing"));
    assert!(field(&registered, "/items/0/data_key").is_string());
    let replay = call(
        &fixture,
        Method::POST,
        &objects,
        &registration,
        StatusCode::CREATED,
    )
    .await;
    for pointer in ["/items/0/object_id", "/items/0/nonce"] {
        assert_eq!(field(&replay, pointer), field(&registered, pointer));
    }
    let upload = json!({"fencing_token": fixture.run.fence, "durable_task": bound.version(),
        "pin_id": text(&plan, "/pin/id"), "objects": [{
            "object_id": text(&registered, "/items/0/object_id"),
            "ciphertext_sha256": "a".repeat(64), "ciphertext_length": 17}]});
    uploads(&fixture, &route, &upload).await;
    fixture.finish().await;
}

/// One plaintext byte uploads as exactly 17 ciphertext bytes: a longer one is
/// refused before signing and one under the floor is malformed, while that
/// length reaches the store, which here cannot establish custody.
async fn uploads(fixture: &Fixture, route: &str, upload: &Value) {
    let route = format!("{route}/uploads");
    let length = "/objects/0/ciphertext_length";
    for (bytes, status, code) in [
        (18, StatusCode::CONFLICT, "UZ-DT-003"),
        (16, StatusCode::BAD_REQUEST, "UZ-REQ-001"),
    ] {
        let mut wrong = upload.clone();
        *wrong.pointer_mut(length).expect("the upload's length") = json!(bytes);
        let refused = call(fixture, Method::POST, &route, &wrong, status).await;
        problem(&refused, code);
    }
    let custody = call(
        fixture,
        Method::POST,
        &route,
        upload,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    problem(&custody, "UZ-DT-009");
}
