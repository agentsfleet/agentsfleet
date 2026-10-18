//! Editing a runner's labels through its patch, over the production schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::mint_id;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::harness::{Fleet, json_body, send};
use crate::integration_operator_runner::{Fixture, SUBJECT, patch, text};

/// A policy unlike the seed's, so a label write that touched it would show.
const POLICY: &str = r#"{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":["registry.example:443"],"worker_count":3,"extra_binds":[]}"#;

/// A second policy, for the edit that must leave labels alone.
const SECOND_POLICY: &str = r#"{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[]}"#;

/// Every column but the two a label write may move, as one comparable string.
const ROW_BUT_LABELS: &str = "\
SELECT (to_jsonb(r) - 'labels' - 'updated_at')::text FROM runner.runners r WHERE r.id = $1::uuid";

/// The stored label column, verbatim.
const STORED_LABELS: &str = "SELECT labels::text FROM runner.runners WHERE id = $1::uuid";

/// The whole row, for the edits that must change nothing.
const WHOLE_ROW: &str = "SELECT to_jsonb(r)::text FROM runner.runners r WHERE r.id = $1::uuid";

/// How many events the runner's history holds, for the no-event repeat.
const EVENT_COUNT: &str = "SELECT count(*)::text FROM runner.events WHERE runner_id = $1::uuid";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn operator_edits_labels_through_the_runner_patch() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();
    let path = format!("/v1/fleets/runners/{}", fixture.runner);

    let enrolled = enrol_with_repeats(&router, &fixture.token).await;
    assert_labels_only_edit(&router, &fixture, &path).await;
    assert_policy_and_labels_save_together(&router, &fixture, &path).await;
    assert_policy_only_edit_keeps_labels(&router, &fixture, &path).await;
    assert_revoked_runner_refuses_the_whole_save(&router, &fixture, &path).await;
    assert_unknown_runner_is_not_found(&router, &fixture.token).await;
    fixture.cleanup(&enrolled).await;
}

/// Enrolment collapses repeats by the same rule the edit uses.
async fn enrol_with_repeats(router: &axum::Router, token: &str) -> String {
    let body = json!({
        "host_id": "operator-edit-enrolment",
        "assigned_policy": serde_json::from_str::<Value>(SECOND_POLICY).expect("fixture policy"),
        "labels": ["gpu", "linux", "gpu"],
    });
    let enrolled = send(
        router,
        Method::POST,
        "/v1/runners",
        Some(token),
        &body.to_string(),
    )
    .await;
    assert_eq!(enrolled.status(), StatusCode::CREATED);
    let runner = text(&json_body(enrolled).await, "runner_id").to_owned();
    assert_eq!(
        read_labels(router, token, &format!("/v1/fleets/runners/{runner}")).await,
        ["gpu", "linux"],
        "enrolment stores each label once, in the order first given"
    );
    runner
}

async fn assert_labels_only_edit(router: &axum::Router, fixture: &Fixture, path: &str) {
    let assigned = patch(
        router,
        &fixture.token,
        path,
        &format!(r#"{{"assigned_policy":{POLICY}}}"#),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);
    let policy_before = column(fixture, ROW_BUT_LABELS).await;

    let edited = patch(
        router,
        &fixture.token,
        path,
        r#"{"labels":["arm64","gpu","arm64","GPU"]}"#,
    )
    .await;
    assert_eq!(edited.status(), StatusCode::OK);
    let edited = json_body(edited).await;
    assert_eq!(edited.get("labels"), Some(&json!(["arm64", "gpu", "GPU"])));
    assert_eq!(
        edited.get("assigned_policy"),
        Some(&Value::Null),
        "no policy was sent"
    );
    assert_eq!(
        read_labels(router, &fixture.token, path).await,
        ["arm64", "gpu", "GPU"],
        "the runner GET reads back exactly what the reply promised"
    );
    assert_eq!(
        column(fixture, STORED_LABELS).await,
        r#"["arm64", "gpu", "GPU"]"#
    );
    assert_eq!(
        column(fixture, ROW_BUT_LABELS).await,
        policy_before,
        "a labels-only edit moved a column other than labels"
    );

    // A repeat changes no byte of the row, `updated_at` included.
    let before = column(fixture, WHOLE_ROW).await;
    let again = patch(
        router,
        &fixture.token,
        path,
        r#"{"labels":["arm64","gpu","GPU"]}"#,
    )
    .await;
    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(column(fixture, WHOLE_ROW).await, before);

    let cleared = patch(router, &fixture.token, path, r#"{"labels":[]}"#).await;
    assert_eq!(cleared.status(), StatusCode::OK);
    assert_eq!(
        column(fixture, STORED_LABELS).await,
        "[]",
        "an empty set clears"
    );
}

/// The Edit dialog's save: both halves land, the reply echoes both, and a
/// repeat of the stored policy appends no event.
async fn assert_policy_and_labels_save_together(
    router: &axum::Router,
    fixture: &Fixture,
    path: &str,
) {
    let body = format!(r#"{{"assigned_policy":{SECOND_POLICY},"labels":["edge","edge","tpu"]}}"#);
    let saved = patch(router, &fixture.token, path, &body).await;
    assert_eq!(saved.status(), StatusCode::OK);
    let saved = json_body(saved).await;
    assert_eq!(saved.get("labels"), Some(&json!(["edge", "tpu"])));
    assert_eq!(
        saved.pointer("/assigned_policy/worker_count"),
        Some(&json!(2))
    );
    assert_eq!(
        read_labels(router, &fixture.token, path).await,
        ["edge", "tpu"]
    );

    let events = column(fixture, EVENT_COUNT).await;
    let repeated = patch(router, &fixture.token, path, &body).await;
    assert_eq!(repeated.status(), StatusCode::OK);
    assert_eq!(
        column(fixture, EVENT_COUNT).await,
        events,
        "a repeated save records no event"
    );
}

async fn assert_policy_only_edit_keeps_labels(
    router: &axum::Router,
    fixture: &Fixture,
    path: &str,
) {
    let labels_before = column(fixture, STORED_LABELS).await;
    for body in [
        format!(r#"{{"assigned_policy":{POLICY}}}"#),
        format!(r#"{{"assigned_policy":{SECOND_POLICY}}}"#),
        r#"{"action":"cordon"}"#.to_owned(),
    ] {
        let patched = patch(router, &fixture.token, path, &body).await;
        assert_eq!(patched.status(), StatusCode::OK, "body={body}");
        assert_eq!(json_body(patched).await.get("labels"), Some(&Value::Null));
        assert_eq!(
            column(fixture, STORED_LABELS).await,
            labels_before,
            "an edit without labels moved them: body={body}"
        );
    }
}

/// A revoked runner refuses a policy, and the labels sent beside it are not
/// written either; labels alone stay editable.
async fn assert_revoked_runner_refuses_the_whole_save(
    router: &axum::Router,
    fixture: &Fixture,
    path: &str,
) {
    let revoked = patch(router, &fixture.token, path, r#"{"action":"revoke"}"#).await;
    assert_eq!(revoked.status(), StatusCode::OK);
    let before = column(fixture, WHOLE_ROW).await;

    let body = format!(r#"{{"assigned_policy":{POLICY},"labels":["after-revoke"]}}"#);
    let refused = patch(router, &fixture.token, path, &body).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        column(fixture, WHOLE_ROW).await,
        before,
        "a refused save wrote half of itself"
    );

    let past_cap: Vec<String> = (0..=afd_wire::runner::LABELS_MAX)
        .map(|index| format!("l{index}"))
        .collect();
    let invalid = json!({ "labels": past_cap }).to_string();
    let refused = patch(router, &fixture.token, path, &invalid).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        column(fixture, WHOLE_ROW).await,
        before,
        "a refused label set wrote something"
    );

    let labelled = patch(router, &fixture.token, path, r#"{"labels":["retired"]}"#).await;
    assert_eq!(
        labelled.status(),
        StatusCode::OK,
        "a revoked runner's labels stay editable"
    );
    assert_eq!(read_labels(router, &fixture.token, path).await, ["retired"]);
}

async fn assert_unknown_runner_is_not_found(router: &axum::Router, token: &str) {
    let path = format!("/v1/fleets/runners/{}", mint_id());
    for body in [
        r#"{"labels":["gpu"]}"#.to_owned(),
        format!(r#"{{"assigned_policy":{POLICY},"labels":[]}}"#),
    ] {
        let missing = patch(router, token, &path, &body).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND, "body={body}");
        assert_eq!(
            json_body(missing)
                .await
                .get("error_code")
                .and_then(Value::as_str),
            Some("UZ-RUN-014")
        );
    }
}

async fn read_labels(router: &axum::Router, token: &str, runner_path: &str) -> Vec<String> {
    let detail = send(router, Method::GET, runner_path, Some(token), "").await;
    assert_eq!(detail.status(), StatusCode::OK);
    json_body(detail)
        .await
        .get("labels")
        .and_then(Value::as_array)
        .expect("the runner GET carries its labels")
        .iter()
        .map(|label| label.as_str().expect("a label is a string").to_owned())
        .collect()
}

async fn column(fixture: &Fixture, statement: &'static str) -> String {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query_scalar(statement)
        .bind(&fixture.runner)
        .fetch_one(&mut *connection)
        .await
        .expect("the seeded runner row")
}
