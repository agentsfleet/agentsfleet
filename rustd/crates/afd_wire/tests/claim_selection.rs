//! A worker advertises exactly the protocols it can execute.

#![expect(
    clippy::expect_used,
    reason = "wire fixtures must parse or fail the test"
)]

use afd_wire::lease::{AcceptedKinds, LeaseKind, LeaseRequest, LeaseResponse};

#[test]
fn test_poll_requires_a_nonempty_distinct_supported_kind_set() {
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
        assert!(
            serde_json::from_str::<LeaseRequest<'_>>(body).is_err(),
            "{body}"
        );
    }
}

#[test]
fn test_poll_kind_order_is_a_set_and_serializes_canonically() {
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
        r#"{"lease":{}}"#,
        r#"{"lease":{"kind":"runtime"}}"#,
        r#"{"lease":{"kind":"authoring"}}"#,
        r#"{"lease":{"kind":"fleet"}}"#,
    ] {
        assert!(
            serde_json::from_str::<LeaseResponse<'_>>(body).is_err(),
            "{body}"
        );
    }
    let empty: LeaseResponse<'_> =
        serde_json::from_str(r#"{"lease":null,"retry_after_ms":250}"#).expect("empty poll");
    assert!(empty.lease.is_none());
    assert_eq!(empty.retry_after_ms, Some(250));
}
