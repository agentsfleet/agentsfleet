//! Empty logical state still crosses native plan, page, pin and recovery guards.

use afd_core::etag::sha256_hex;
use afd_crypto::checkpoint_digest;
use afd_db::test_util::mint_uuid7;
use afd_wire::task::CheckpointPage;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, call, foreign_and_stale, path, problem, success};

pub(super) async fn exercise(fixture: &Fixture, task: &Value) {
    let route = path(fixture, "checkpoints");
    let input = json!({"fencing_token":fixture.lease.fence,"operation_id":mint_uuid7(),
        "task":task,"mode":"incremental","base_id":null,"commit_oid":null,
        "format_version":1,"toolbox_digest":"3".repeat(64),"page_count":1,
        "upload_budget_bytes":0,"logical_bytes":0,"entry_count":0});
    foreign_and_stale(fixture, &route, &input).await;
    let plan = call(fixture, Method::POST, &route, &input, StatusCode::CREATED).await;
    assert_eq!(plan["replayed"], false);
    let replay = call(fixture, Method::POST, &route, &input, StatusCode::OK).await;
    assert_eq!(replay["checkpoint_id"], plan["checkpoint_id"]);
    assert_eq!(replay["replayed"], true);
    let checkpoint = plan["checkpoint_id"].as_str().expect("native checkpoint");
    let route = format!("{route}/{checkpoint}");
    renew_pin(fixture, task, &route, &plan).await;
    let (contents, digest) = write_page(fixture, task, &route, &plan).await;
    let commit = commit(fixture, task, &route, &plan, &digest).await;
    restore(fixture, task, &route, &plan, &contents, &commit).await;
    register_and_refuse_bad_upload(fixture, task, &input).await;
}

async fn register_and_refuse_bad_upload(fixture: &Fixture, task: &Value, original: &Value) {
    let collection = path(fixture, "checkpoints");
    let mut input = original.clone();
    input["operation_id"] = json!(mint_uuid7());
    input["upload_budget_bytes"] = json!(64);
    let plan = call(
        fixture,
        Method::POST,
        &collection,
        &input,
        StatusCode::CREATED,
    )
    .await;
    let route = format!(
        "{collection}/{}",
        plan["checkpoint_id"].as_str().expect("second capture")
    );
    let registration = json!({"fencing_token":fixture.lease.fence,"task":task,
        "pin_id":plan["pin"]["id"],"objects":[{"sha256":sha256_hex(b"x"),"size":1,"kind":"file"}]});
    let objects = call(
        fixture,
        Method::POST,
        &format!("{route}/objects"),
        &registration,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(objects["items"][0]["status"], "missing");
    assert!(objects["items"][0]["data_key"].is_string());
    let replay = call(
        fixture,
        Method::POST,
        &format!("{route}/objects"),
        &registration,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        replay["items"][0]["object_id"],
        objects["items"][0]["object_id"]
    );
    assert_eq!(replay["items"][0]["nonce"], objects["items"][0]["nonce"]);
    refuse_upload(fixture, task, &route, &plan, &objects).await;
}

async fn renew_pin(fixture: &Fixture, task: &Value, route: &str, plan: &Value) {
    let pin = plan["pin"]["id"].as_str().expect("writer pin");
    let renewal = json!({"fencing_token":fixture.lease.fence,"task":task,
        "resource_version":plan["pin"]["resource_version"]});
    let renewed = call(
        fixture,
        Method::PATCH,
        &format!("{route}/pins/{pin}"),
        &renewal,
        StatusCode::OK,
    )
    .await;
    assert!(
        renewed["resource_version"].as_u64().expect("pin version")
            > plan["pin"]["resource_version"]
                .as_u64()
                .expect("original version")
    );
    let refused = call(
        fixture,
        Method::PATCH,
        &format!("{route}/pins/{pin}"),
        &renewal,
        StatusCode::CONFLICT,
    )
    .await;
    problem(&refused, "UZ-DT-002");
}

async fn write_page(
    fixture: &Fixture,
    task: &Value,
    route: &str,
    plan: &Value,
) -> (CheckpointPage, String) {
    let pin = plan["pin"]["id"].as_str().expect("writer pin");
    let contents = CheckpointPage {
        page_number: 0,
        entries: vec![],
        history_objects: vec![],
    };
    let digest = sha256_hex(&serde_json::to_vec(&contents).expect("canonical page"));
    let page = plan["page_ids"][0].as_str().expect("allocated page");
    let page_route = format!("{route}/pages/{page}");
    let write = json!({"fencing_token":fixture.lease.fence,"task":task,"pin_id":pin,
        "digest":digest,"contents":contents});
    for _ in 0..2 {
        let stored = call(fixture, Method::PUT, &page_route, &write, StatusCode::OK).await;
        assert_eq!(
            stored,
            serde_json::to_value(&contents).expect("same complete page")
        );
    }
    let mut corrupt = write.clone();
    corrupt["digest"] = json!("f".repeat(64));
    let refused = call(
        fixture,
        Method::PUT,
        &page_route,
        &corrupt,
        StatusCode::CONFLICT,
    )
    .await;
    problem(&refused, "UZ-DT-008");
    (contents, digest)
}

async fn commit(fixture: &Fixture, task: &Value, route: &str, plan: &Value, digest: &str) -> Value {
    let pin = plan["pin"]["id"].as_str().expect("writer pin");
    let page = plan["page_ids"][0].as_str().expect("allocated page");
    let commit = json!({"fencing_token":fixture.lease.fence,"task":task,"pin_id":pin,
        "manifest_root":checkpoint_digest::manifest_root([(page,digest)]),
        "history_digest":checkpoint_digest::history_root([]),"covered_input_sequence":0,
        "effect_watermark":0});
    let saved = call(fixture, Method::PATCH, route, &commit, StatusCode::OK).await;
    assert_eq!(saved["manifest_root"], commit["manifest_root"]);
    assert_eq!(saved["replayed"], false);
    let replay = call(fixture, Method::PATCH, route, &commit, StatusCode::OK).await;
    assert_eq!(replay["replayed"], true);
    commit
}

async fn restore(
    fixture: &Fixture,
    task: &Value,
    route: &str,
    plan: &Value,
    contents: &CheckpointPage,
    commit: &Value,
) {
    let page = plan["page_ids"][0].as_str().expect("allocated page");
    let page_route = format!("{route}/pages/{page}");
    let restore = json!({"fencing_token":fixture.lease.fence,"task":task,
        "purpose":"restore","pin_id":null,"object_ids":[]});
    let restored = call(
        fixture,
        Method::POST,
        &format!("{route}/authorizations"),
        &restore,
        StatusCode::OK,
    )
    .await;
    assert_eq!(restored["plan"]["manifest_root"], commit["manifest_root"]);
    assert_eq!(restored["objects"], json!([]));
    let reader = restored["plan"]["pin"]["id"].as_str().expect("restore pin");
    let query = format!("fencing_token={}&pin_id={reader}", fixture.lease.fence);
    let read = success(
        fixture,
        Method::GET,
        &format!("{page_route}?{query}"),
        "",
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        read,
        serde_json::to_value(&contents).expect("restored page")
    );
    let objects = success(
        fixture,
        Method::GET,
        &format!("{route}/objects?{query}"),
        "",
        StatusCode::OK,
    )
    .await;
    assert_eq!(objects, json!({"items":[],"next_cursor":null}));
}

async fn refuse_upload(
    fixture: &Fixture,
    task: &Value,
    route: &str,
    plan: &Value,
    objects: &Value,
) {
    let upload = json!({"fencing_token":fixture.lease.fence,"task":task,
        "pin_id":plan["pin"]["id"],"objects":[{"object_id":objects["items"][0]["object_id"],
        "ciphertext_sha256":"a".repeat(64),"ciphertext_length":18}]});
    // One plaintext byte requires exactly 17 ciphertext bytes; refuse before signing or I/O.
    let refused = call(
        fixture,
        Method::POST,
        &format!("{route}/uploads"),
        &upload,
        StatusCode::CONFLICT,
    )
    .await;
    problem(&refused, "UZ-DT-003");
}
