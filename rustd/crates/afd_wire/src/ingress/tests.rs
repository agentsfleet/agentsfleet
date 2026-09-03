//! Each untagged answer serializes as the document it wraps, and no more.

use std::borrow::Cow;

use super::{
    AccountOpened, AppIngressAnswer, EchoAnswer, EventsAnswer, IdentityAnswer, Ignored, Pong,
};

/// The untagged answer is bytes-identical to the document it wraps.
///
/// Asserted as bytes: a vendor reads the echo literally, and a wrapper
/// or a tag around it fails the ownership check on their side.
#[test]
fn test_the_events_answer_adds_no_bytes_around_either_document() {
    let echo = EventsAnswer::Echo(EchoAnswer {
        field: std::iter::once(("challenge", "3eZbrw1a")).collect(),
    });
    let ignored = EventsAnswer::Ignored(Ignored {
        ignored: Cow::Borrowed("fleet_paused"),
    });

    assert_eq!(
        serde_json::to_string(&echo).ok().as_deref(),
        Some(r#"{"challenge":"3eZbrw1a"}"#),
    );
    assert_eq!(
        serde_json::to_string(&ignored).ok().as_deref(),
        Some(r#"{"ignored":"fleet_paused"}"#),
    );
}

/// The other two untagged answers add no bytes either.
#[test]
fn test_the_ingress_and_identity_answers_add_no_bytes_around_their_documents() {
    let pong = AppIngressAnswer::Pong(Pong {
        status: Cow::Borrowed("ok"),
    });
    let dropped = AppIngressAnswer::Ignored(Ignored {
        ignored: Cow::Borrowed("fleet_paused"),
    });
    let opened = IdentityAnswer::Opened(AccountOpened {
        workspace_id: Cow::Borrowed("01924f4e-0000-7000-8000-000000000001"),
        workspace_name: Cow::Borrowed("acme"),
        created: true,
    });

    assert_eq!(
        serde_json::to_string(&pong).ok().as_deref(),
        Some(r#"{"status":"ok"}"#)
    );
    assert_eq!(
        serde_json::to_string(&dropped).ok().as_deref(),
        Some(r#"{"ignored":"fleet_paused"}"#)
    );
    assert_eq!(
        serde_json::to_string(&opened).ok().as_deref(),
        Some(
            r#"{"workspace_id":"01924f4e-0000-7000-8000-000000000001","workspace_name":"acme","created":true}"#
        )
    );
}
