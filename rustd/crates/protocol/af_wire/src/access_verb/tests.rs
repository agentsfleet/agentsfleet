#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use garde::Validate as _;

use super::{AccessRequest, AccessRequested};

#[test]
fn a_github_request_reads_its_scope() {
    let request: AccessRequest<'_> = serde_json::from_str(
        r#"{"fencing_token": 3, "name": "github",
            "scope": {"repositories": ["org/app"], "access": "read"}}"#,
    )
    .expect("a scoped request");

    assert_eq!(request.name, "github");
    request
        .validate()
        .expect("a scoped request holds its bounds");
    assert_eq!(
        request
            .scope
            .as_ref()
            .and_then(|scope| scope.get("access"))
            .and_then(serde_json::Value::as_str),
        Some("read")
    );
}

#[test]
fn an_empty_or_nul_name_is_refused() {
    for name in ["", "sen\u{0}try"] {
        let request = AccessRequest {
            fencing_token: 1,
            name: name.into(),
            scope: None,
        };
        assert!(request.validate().is_err(), "{name:?}");
    }
}

#[test]
fn a_scope_that_is_not_an_object_or_an_unknown_key_is_refused() {
    for body in [
        r#"{"fencing_token": 3, "name": "github", "scope": ["org/app"]}"#,
        r#"{"fencing_token": 3, "name": "github", "scope": "org/app"}"#,
        r#"{"fencing_token": 3, "name": "sentry", "host": "sentry.io"}"#,
        r#"{"name": "sentry"}"#,
    ] {
        assert!(
            serde_json::from_str::<AccessRequest<'_>>(body).is_err(),
            "{body}"
        );
    }
}

#[test]
fn a_covered_request_carries_no_card() {
    let covered: AccessRequested<'_> =
        serde_json::from_str(r#"{"grant_id": "g", "gate_id": null}"#).expect("covered");
    assert!(covered.gate_id.is_none());
}
