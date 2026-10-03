#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::error_code;

use super::{Error, ErrorKind, raise};
use crate::client::Verb;

const VERBS: [Verb; 9] = [
    Verb::Heartbeat,
    Verb::Lease,
    Verb::Renew,
    Verb::Activity,
    Verb::Report,
    Verb::Hydrate,
    Verb::Capture,
    Verb::Bundle,
    Verb::Mint,
];

#[test]
fn every_verb_logs_under_its_own_family() {
    let codes: Vec<_> = VERBS.iter().map(|verb| verb.code()).collect();

    assert_eq!(codes[7], error_code::FLEET_BUNDLE_FETCH_FAILED);
    assert_eq!(codes[5], error_code::MEM_UNAVAILABLE);
    assert_eq!(codes[6], error_code::MEM_UNAVAILABLE);
    let internal = codes
        .iter()
        .filter(|code| **code == error_code::INTERNAL_OPERATION_FAILED)
        .count();
    assert_eq!(internal, 6);
    assert_eq!(Verb::Renew.to_string(), "renew");
    for verb in VERBS {
        assert_eq!(verb.to_string(), verb.as_str());
        assert!(
            verb.as_str().bytes().all(|byte| byte.is_ascii_lowercase()),
            "{verb:?} logs in lower case"
        );
    }
    assert!(Verb::Bundle.reads() && Verb::Hydrate.reads() && !Verb::Report.reads());
}

#[test]
fn a_refusal_carries_the_daemons_code_and_status_through() {
    let body = br#"{"error_code":"UZ-RUN-015","detail":"over budget"}"#;
    let refusal = raise::refused_with_body(Verb::Renew, 402, body);

    assert_eq!(
        refusal.refusal_code(),
        Some(error_code::RUN_BUDGET_EXCEEDED)
    );
    assert_eq!(refusal.refusal_status(), Some(402));
    assert_eq!(refusal.code(), error_code::RUN_BUDGET_EXCEEDED);
    assert!(!refusal.is_retryable() && !refusal.is_not_found());
    assert!(
        refusal.to_string().contains("refused the renew call (402)"),
        "{refusal}"
    );
}

#[test]
fn a_refusal_without_a_known_code_falls_back_by_status_then_verb() {
    let unknown = raise::refused_with_body(Verb::Renew, 409, br#"{"error_code":"UZ-NOPE-999"}"#);
    let unreadable = raise::refused_with_body(Verb::Report, 400, b"<html>");
    let unauthorized = raise::refused(Verb::Lease, 401, None);
    let absent = raise::refused(Verb::Bundle, 404, None);

    assert_eq!(unknown.code(), error_code::RUN_LEASE_LOST);
    assert_eq!(unreadable.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(unreadable.refusal_code(), None);
    assert!(unauthorized.is_unauthorized());
    assert_eq!(unauthorized.code(), error_code::RUN_INVALID_RUNNER_TOKEN);
    assert!(absent.is_not_found() && !absent.is_unauthorized());
}

#[test]
fn a_stopped_runner_names_its_refused_token() {
    let stopped = raise::token_refused();

    assert!(stopped.is_unauthorized());
    assert_eq!(stopped.code(), error_code::RUN_INVALID_RUNNER_TOKEN);
    assert_eq!(stopped.refusal_status(), None);
}

#[test]
fn a_blip_is_retryable_and_names_its_verb_family() {
    let busy = raise::unavailable(Verb::Hydrate, 503);

    assert!(busy.is_retryable());
    assert!(!busy.is_unauthorized());
    assert_eq!((busy.refusal_code(), busy.refusal_status()), (None, None));
    assert_eq!(busy.code(), error_code::MEM_UNAVAILABLE);
}

#[tokio::test]
async fn a_transport_failure_is_retryable() {
    let unreachable = reqwest::Client::new()
        .get("http://127.0.0.1:1/")
        .send()
        .await
        .unwrap_err();
    let failure = raise::transport(Verb::Bundle)(unreachable);

    assert!(failure.is_retryable());
    assert_eq!(failure.code(), error_code::FLEET_BUNDLE_FETCH_FAILED);
    assert!(std::error::Error::source(&failure).is_some());
}

#[test]
fn local_failures_log_as_internal() {
    let decode = serde_json::from_str::<u8>("x").unwrap_err();
    let encode = serde_json::from_str::<u8>("y").unwrap_err();
    let persist = tempfile::NamedTempFile::new()
        .unwrap()
        .persist("/no/such/dir/x")
        .unwrap_err();
    let unbuildable = reqwest::Client::builder()
        .user_agent("\n")
        .build()
        .unwrap_err();
    let failures: Vec<Error> = vec![
        raise::malformed(Verb::Lease)(decode),
        raise::encode(encode),
        raise::config("unset"),
        url::Url::parse("no scheme").unwrap_err().into(),
        raise::client(unbuildable),
        std::io::Error::other("disk").into(),
        persist.into(),
        afd_core::id::Uuid7::parse("nope").unwrap_err().into(),
    ];

    for failure in &failures {
        assert_eq!(
            failure.code(),
            error_code::INTERNAL_OPERATION_FAILED,
            "{failure}"
        );
        assert!(!failure.is_retryable());
    }
    assert_eq!(
        raise::tampered("abc").code(),
        error_code::FLEET_BUNDLE_INVALID
    );
    assert!(matches!(
        raise::tampered("abc").kind(),
        ErrorKind::BundleTampered { .. }
    ));
}
