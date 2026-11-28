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

    assert_eq!(request.name.as_deref(), Some("github"));
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
            name: Some(name.into()),
            host: None,
            evidence: None,
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
        r#"{"fencing_token": 3, "name": "sentry", "typo": "sentry.io"}"#,
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
        serde_json::from_str(r#"{"state":"covered","grant_id": "g", "gate_id": null}"#)
            .expect("covered");
    assert!(covered.gate_id.is_none());
}

#[test]
fn only_bare_dns_names_can_become_canonical_host_grants() {
    for invalid in [
        "10.0.0.5",
        "169.254.169.254",
        "[::1]",
        "127.1",
        "0x7f.1",
        "localhost",
        "app.localhost.",
        "intranet",
        "example.org:443",
        "https://example.org",
        "user@example.org",
        "example.org/page",
        "example.org?secret",
        "example.org#private",
        "a..example",
        "*.example.org",
        "bad_label.example",
        "example.%6frg",
    ] {
        assert!(super::Hostname::parse(invalid).is_err(), "{invalid}");
    }
    let host = super::Hostname::parse("BÜCHER.Example.").expect("an international DNS name");
    assert_eq!(host.as_str(), "xn--bcher-kva.example");
}

#[test]
fn a_host_cannot_borrow_evidence_from_a_different_destination_or_tool() {
    let valid = serde_json::json!({
        "fencing_token": 3, "host":"docs.example",
        "evidence":{"tool":"web_fetch","from_host":"app.example","to_host":"DOCS.example"},
    });
    let parse = |body: &serde_json::Value| {
        serde_json::from_str::<AccessRequest<'_>>(&body.to_string())
            .is_ok_and(|r| r.validate().is_ok())
    };
    assert!(parse(&valid));
    for (path, value) in [
        ("tool", "http_request"),
        ("from_host", "https://app.example/?signed"),
        ("to_host", "other.example"),
        ("path", "/private"),
    ] {
        let mut invalid = valid.clone();
        invalid
            .get_mut("evidence")
            .and_then(serde_json::Value::as_object_mut)
            .expect("the valid request has evidence")
            .insert(path.to_owned(), value.into());
        assert!(!parse(&invalid), "{path}");
    }
}
