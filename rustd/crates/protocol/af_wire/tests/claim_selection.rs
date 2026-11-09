//! A worker advertises exactly the protocols it can execute.

#![expect(
    clippy::expect_used,
    reason = "wire fixtures must parse or fail the test"
)]

use af_wire::{
    lease::{AcceptedKinds, LeaseKind, TaggedLease},
    protocol::{Down, Up},
};

#[test]
fn hello_requires_a_nonempty_distinct_supported_kind_set() {
    for body in [
        "{}",
        r#"{"accepted_kinds":null}"#,
        r#"{"accepted_kinds":[]}"#,
        r#"{"accepted_kinds":["fleet","fleet"]}"#,
        r#"{"accepted_kinds":["authoring","authoring"]}"#,
        r#"{"accepted_kinds":["fleet","authoring","fleet"]}"#,
        r#"{"accepted_kinds":["runtime"]}"#,
        r#"{"accepted_kinds":"fleet"}"#,
    ] {
        let mut message: serde_json::Value = serde_json::from_str(body).expect("fixture object");
        let header = serde_json::json!({"type":"hello","request_id":1,"running":[],"credits":0});
        message
            .as_object_mut()
            .expect("fixture object")
            .extend(header.as_object().expect("header").clone());
        let text = message.to_string();
        assert!(serde_json::from_str::<Up<'_>>(&text).is_err(), "{text}");
    }
}

#[test]
fn accepted_kind_order_is_a_set_and_serializes_canonically() {
    for (text, expected) in [
        (r#"["fleet"]"#, AcceptedKinds::FLEET),
        (r#"["authoring"]"#, AcceptedKinds::AUTHORING),
        (r#"["fleet","authoring"]"#, AcceptedKinds::ALL),
        (r#"["authoring","fleet"]"#, AcceptedKinds::ALL),
    ] {
        let actual: AcceptedKinds = serde_json::from_str(text).expect("supported set");
        assert_eq!(actual, expected);
        let round_trip: AcceptedKinds =
            serde_json::from_value(serde_json::to_value(actual).expect("serializable set"))
                .expect("canonical set decodes");
        assert_eq!(round_trip, expected);
    }
    assert!(AcceptedKinds::ALL.contains(LeaseKind::Fleet));
    assert!(AcceptedKinds::ALL.contains(LeaseKind::Authoring));
    assert!(!AcceptedKinds::FLEET.contains(LeaseKind::Authoring));
    assert_eq!(
        serde_json::to_value(AcceptedKinds::ALL).expect("serializable set"),
        serde_json::json!(["fleet", "authoring"])
    );
}

#[test]
fn test_claim_reply_requires_a_supported_discriminator() {
    for body in [
        "{}",
        r#"{"kind":"runtime"}"#,
        r#"{"kind":"authoring"}"#,
        r#"{"kind":"fleet"}"#,
    ] {
        assert!(
            serde_json::from_str::<TaggedLease<'_>>(body).is_err(),
            "{body}"
        );
    }
    serde_json::from_str::<Down<'_>>(r#"{"type":"offer","lease":null}"#)
        .expect_err("an offer must contain a native lease");
}
