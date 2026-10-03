//! What a schedule's fields are bounded to, proved through the whole layer
//! stack before any store is reached.
//!
//! The bounds and the readers they protect are unit-tested in
//! `afd_cron/tests/validate.rs`. What this adds is that each refusal arrives as
//! the code and sentence a client branches on, with no Postgres behind the
//! router: every case here is refused before the store would be asked.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_cron::validate::{MAX_CRON_LEN, MAX_MESSAGE_LEN, MAX_TIMEZONE_LEN};
use http::{Method, StatusCode};
use serde_json::{Value, json};

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_t5c4ed01e5c4ed01e5c4ed01e5c4ed01e5c4ed01e5c4ed01e5c4ed01e5c4ed01e";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2schedules_input";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee8";

/// An expression this daemon registers.
const NIGHTLY: &str = "0 3 * * *";

/// The sentence an expression this daemon will not register earns.
// pin test: literal is the contract
const DETAIL_INVALID_CRON: &str =
    "The cron expression must be five numeric fields this daemon accepts.";

/// The sentence a zone this daemon will not pass upstream earns.
// pin test: literal is the contract
const DETAIL_INVALID_TIMEZONE: &str = "The timezone is not a name this daemon will register.";

/// One create, as a person holding the schedule-write rung sends it.
async fn create(body: &Value) -> (StatusCode, Value) {
    let router = Fleet::new()
        .with_person(
            TENANT_KEY,
            SUBJECT,
            ScopeSet::from_scopes(&[Scope::ScheduleWrite]),
        )
        .router();
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/schedules");
    let response = harness::send(
        &router,
        Method::POST,
        &path,
        Some(TENANT_KEY),
        &body.to_string(),
    )
    .await;
    let status = response.status();
    (status, harness::json_body(response).await)
}

/// The `error_code` and `detail` a refusal carries.
fn refusal(document: &Value) -> (&str, &str) {
    let field = |name: &str| {
        document
            .get(name)
            .and_then(Value::as_str)
            .expect("every refusal carries the field the case reads")
    };
    (field("error_code"), field("detail"))
}

#[tokio::test]
async fn test_schedule_message_over_cap_names_the_cap() {
    let over = "m".repeat(MAX_MESSAGE_LEN + 1);
    let (status, document) = create(&json!({ "cron": NIGHTLY, "message": over })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (code, detail) = refusal(&document);
    assert_eq!(code, error_code::INVALID_REQUEST.as_str());
    assert!(
        detail.contains(&MAX_MESSAGE_LEN.to_string()),
        "an oversized message is told its cap, not that it is empty: {detail}"
    );
}

#[tokio::test]
async fn an_oversized_expression_and_zone_keep_their_sentences() {
    let expression = "0".repeat(MAX_CRON_LEN + 1);
    let (status, document) = create(&json!({ "cron": expression, "message": "run" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refusal(&document),
        (error_code::INVALID_REQUEST.as_str(), DETAIL_INVALID_CRON)
    );

    let zone = "A".repeat(MAX_TIMEZONE_LEN + 1);
    let (status, document) = create(&json!({
        "cron": NIGHTLY,
        "timezone": zone,
        "message": "run",
    }))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refusal(&document),
        (
            error_code::INVALID_REQUEST.as_str(),
            DETAIL_INVALID_TIMEZONE
        )
    );
}
