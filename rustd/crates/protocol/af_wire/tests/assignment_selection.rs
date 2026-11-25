//! Every runner runs fleets: a hello names no work kinds, and an assignment
//! carries a fleet's turn.

#![expect(
    clippy::expect_used,
    reason = "wire fixtures must parse or fail the test"
)]

use af_wire::{
    protocol::{Down, Up},
    run::TaggedRun,
};

/// The hello's fields, without any kind set.
fn hello() -> serde_json::Value {
    serde_json::json!({"type":"hello","request_id":1,"running":[],"credits":0})
}

#[test]
fn a_hello_without_kinds_decodes() {
    let text = hello().to_string();
    assert!(
        matches!(serde_json::from_str::<Up<'_>>(&text), Ok(Up::Hello { .. })),
        "{text}"
    );
}

#[test]
fn a_hello_naming_any_kind_set_is_refused() {
    for kinds in [
        serde_json::json!(["fleet"]),
        serde_json::json!(["fleet", "setup"]),
        serde_json::json!([]),
        serde_json::Value::Null,
    ] {
        let mut message = hello();
        message
            .as_object_mut()
            .expect("fixture object")
            .insert("accepted_kinds".to_owned(), kinds);
        let text = message.to_string();
        assert!(serde_json::from_str::<Up<'_>>(&text).is_err(), "{text}");
    }
}

#[test]
fn test_assignment_reply_requires_a_supported_discriminator() {
    for body in [
        "{}",
        r#"{"kind":"runtime"}"#,
        r#"{"kind":"authoring"}"#,
        r#"{"kind":"setup"}"#,
        r#"{"kind":"fleet"}"#,
    ] {
        assert!(
            serde_json::from_str::<TaggedRun<'_>>(body).is_err(),
            "{body}"
        );
    }
    serde_json::from_str::<Down<'_>>(r#"{"type":"run","run":null}"#)
        .expect_err("an assignment must contain a native run");
}
