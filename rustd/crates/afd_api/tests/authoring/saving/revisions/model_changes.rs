//! Owner-approved model changes are history; current readiness governs resume.

use super::super::super::fixture::Fixture;
use super::{
    Fleet, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};
use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_dragonfly::{
    Subscription, SubscriptionHub,
    config::{DragonflyConfig, DragonflyRole},
    hub::Received,
    streams::{FleetStreams, fleet_activity_channel},
};
use std::time::Duration;

const DELIVERY_BUDGET: Duration = Duration::from_secs(5);
const PROBE: &str = "model-change-subscription-ready";
const FINISHED: &str = "model-change-writes-finished";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn saved_model_changes_record_one_notice_even_when_the_save_is_replayed() {
    let fleet = Fleet::create().await;
    let (hub, mut reader) = subscribe(&fleet).await;
    let other = Fixture::create().await;
    let selected = mint();
    sqlx::query("INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,created_at,updated_at) SELECT $1::uuid,$2::uuid,model_name,'authoring-model',now(),now() FROM model.tenant_models WHERE id=$3::uuid")
        .bind(selected.as_str()).bind(fleet.case.base.tenant.as_str()).bind(other.model.as_str())
        .execute(&mut *fleet.case.base.db.acquire().await.expect("model connection"))
        .await.expect("second runtime choice");
    let (draft, _) = fleet.improvement().await;
    let request = select_model(&fleet, &draft, selected.as_str()).await;
    let key = mint();
    let tag = fleet.saved["setup_etag"].as_str().expect("saved setup tag");
    let applied = fleet.apply(key.as_str(), tag, &request).await;
    assert_eq!(applied.status(), StatusCode::CREATED);
    let applied = json_body(applied).await;
    let notice: Value = serde_json::from_str(&receive(&mut reader).await).expect("live notice");
    assert_eq!(notice, json!({"kind": "catching_up", "dropped": 0}));
    assert_eq!(fleet.current().await["setup_revision"], "2");
    assert_eq!(
        fleet.apply(key.as_str(), tag, &request).await.status(),
        StatusCode::CREATED
    );
    assert_model_notice(&fleet, selected.as_str()).await;
    save_without_model_change(&fleet, &applied).await;
    FleetStreams::new(fleet.case.queue.clone())
        .publish_tail(fleet.saved["id"].as_str().expect("fleet ID"), FINISHED)
        .await
        .expect("ordered end marker");
    assert_eq!(
        receive(&mut reader).await,
        FINISHED,
        "replay and unchanged model publish no extra notice"
    );
    drop(reader);
    hub.shutdown();
    fleet.case.cleanup().await;
    other.cleanup().await;
}

async fn subscribe(fleet: &Fleet) -> (SubscriptionHub, Subscription) {
    let config = DragonflyConfig::from_url(
        DragonflyRole::Default,
        ProcessEnv
            .get("TEST_DRAGONFLY_URL")
            .expect("live cluster URL"),
    );
    let hub = SubscriptionHub::start(config)
        .await
        .expect("live subscription hub");
    let id = fleet.saved["id"].as_str().expect("fleet ID");
    let mut reader = hub.subscribe(&fleet_activity_channel(id));
    let publisher = FleetStreams::new(fleet.case.queue.clone());
    tokio::time::timeout(DELIVERY_BUDGET, async {
        while publisher
            .publish_tail(id, PROBE)
            .await
            .expect("subscription probe")
            == 0
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("subscription confirmed before model change");
    assert_eq!(receive(&mut reader).await, PROBE);
    (hub, reader)
}

async fn receive(reader: &mut Subscription) -> String {
    let received = tokio::time::timeout(DELIVERY_BUDGET, reader.recv())
        .await
        .expect("live notice within budget")
        .expect("subscription remains open");
    let Received::Message(message) = received else {
        panic!("a published message, not a gap");
    };
    message.payload.clone()
}

async fn assert_model_notice(fleet: &Fleet, selected: &str) {
    let thread = events(fleet, "messages").await;
    let notices = thread["items"].as_array().expect("thread items");
    assert_eq!(notices.len(), 1, "replayed save records one notice");
    assert_eq!(notices[0]["event_type"], "model_changed");
    assert_eq!(notices[0]["actor"], "config_reload");
    assert_eq!(notices[0]["status"], "processed");
    assert!(
        notices[0]["response_text"].is_null(),
        "settings do not run inference"
    );
    let payload: Value =
        serde_json::from_str(notices[0]["request_json"].as_str().expect("notice JSON"))
            .expect("model notice");
    assert_eq!(payload["model_ref"], selected);
    assert_eq!(
        payload["previous_model_ref"],
        fleet.case.base.model.as_str()
    );
    assert_eq!(
        events(fleet, "events").await["items"][0]["event_type"],
        "model_changed"
    );
}

async fn save_without_model_change(fleet: &Fleet, applied: &Value) {
    let (_, unchanged_model) = fleet.improvement_of(&json!("2")).await;
    let saved = fleet
        .apply(
            mint().as_str(),
            applied["setup_etag"].as_str().expect("current tag"),
            &unchanged_model,
        )
        .await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    assert_eq!(
        events(fleet, "messages").await["items"]
            .as_array()
            .expect("thread items")
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn catalogue_price_changes_do_not_require_review_to_resume_a_ready_fleet() {
    let fleet = Fleet::create().await;
    let stopped = stop_then_change_price(&fleet).await;
    let resumed = fleet
        .patch_resource(
            r#"{"status":"active"}"#,
            stopped["etag"].as_str().expect("stopped tag"),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed = json_body(resumed).await;
    assert_eq!(
        resumed["status"], "active",
        "ready setup needs no repeat review"
    );
    assert!(resumed.get("needs_re_review").is_none());
    assert!(
        !resumed["setup"]["checks"]
            .as_array()
            .expect("current checks")
            .iter()
            .any(|check| check["subject"] == "review_changed")
    );
    assert!(
        events(&fleet, "messages").await["items"]
            .as_array()
            .expect("thread items")
            .is_empty(),
        "price drift is not a model change"
    );
    fleet.case.cleanup().await;
}

async fn stop_then_change_price(fleet: &Fleet) -> Value {
    let stopped = fleet
        .patch_resource(
            r#"{"status":"stopped"}"#,
            fleet.saved["etag"].as_str().expect("fleet tag"),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::OK);
    sqlx::query("UPDATE model.platform_model_prices SET input_nanos_per_mtok=input_nanos_per_mtok+1,updated_at=clock_timestamp() WHERE platform_model_id=$1::uuid")
        .bind(fleet.case.base.catalogue.as_str())
        .execute(&mut *fleet.case.base.db.acquire().await.expect("price connection"))
        .await.expect("catalogue price changed");
    json_body(stopped).await
}

async fn select_model(fleet: &Fleet, draft: &str, model: &str) -> Value {
    let current = send(
        &fleet.case.router,
        Method::GET,
        draft,
        Some(&fleet.case.base.token),
        "",
    )
    .await;
    let tag = current.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let edited = send_with_headers(
        &fleet.case.router,
        Method::PATCH,
        draft,
        Some(&fleet.case.base.token),
        &json!({"runtime_model_ref":model}).to_string(),
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    let status = edited.status();
    let edited = json_body(edited).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    json!({"draft_candidate_id":edited["candidate"]["id"],"review_digest":edited["review"]["review_digest"]})
}

async fn events(fleet: &Fleet, suffix: &str) -> Value {
    let response = send(
        &fleet.case.router,
        Method::GET,
        &format!("{}/{suffix}", fleet.item),
        Some(&fleet.case.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_failed_live_notice_keeps_the_model_save_committed_and_replayable() {
    let mut fleet = Fleet::create().await;
    let other = Fixture::create().await;
    let (draft, _) = fleet.improvement().await;
    let selected = mint();
    sqlx::query("INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,created_at,updated_at) SELECT $1::uuid,$2::uuid,model_name,'authoring-model',now(),now() FROM model.tenant_models WHERE id=$3::uuid")
        .bind(selected.as_str()).bind(fleet.case.base.tenant.as_str()).bind(other.model.as_str())
        .execute(&mut *fleet.case.base.db.acquire().await.expect("model connection"))
        .await.expect("second runtime choice");
    let request = select_model(&fleet, &draft, selected.as_str()).await;
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned())
            .with_request_timeout(Duration::from_millis(200));
    let queue = afd_dragonfly::Dragonfly::unreachable(&config).expect("lazy unavailable queue");
    fleet.case.router = fleet
        .case
        .base
        .services(&afd_auth::Scope::ALL)
        .with_fleet_queue_budgets(
            fleet.case.base.db.clone(),
            queue,
            afd_admission::Budgets::default(),
        )
        .router();
    let key = mint();
    let tag = fleet.saved["setup_etag"].as_str().expect("saved setup tag");
    let applied = fleet.apply(key.as_str(), tag, &request).await;
    assert_eq!(applied.status(), StatusCode::CREATED);
    assert_eq!(json_body(applied).await["setup_revision"], "2");
    assert_eq!(
        fleet.current().await["definition"]["config"]["x-agentsfleet"]["model"],
        selected.as_str()
    );
    assert_eq!(
        fleet.apply(key.as_str(), tag, &request).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(fleet.revision_count().await, 2);
    assert_model_notice(&fleet, selected.as_str()).await;
    fleet.case.cleanup().await;
    other.cleanup().await;
}
