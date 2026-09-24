use super::*;

/// The verdict a real response earns, through both halves of the split.
fn verdict(status: u16, payload: &str) -> Verdict {
    classify(status, payload).unwrap_or_else(|_event| verdict_of(status))
}

/// A job carrying `address`, with every other field well formed.
fn job(address: &str) -> OutboundDelivery {
    OutboundDelivery {
        id: afd_dragonfly::streams::EventId::of("1700000000001-0"),
        provider: Provider::Slack.id().to_owned(),
        destination: address.to_owned(),
        workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
        fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
        event_id: "1700000000000-0".to_owned(),
        answer: "the answer".to_owned(),
    }
}

#[test]
fn test_only_a_200_that_slack_accepted_is_delivered() {
    assert_eq!(
        verdict(200, r#"{"ok":true,"ts":"1700000000.000100"}"#),
        Verdict::Delivered
    );
}

/// The case a status check alone gets wrong: Slack refuses at the
/// application layer with a 200.
#[test]
fn test_a_200_carrying_ok_false_is_permanent() {
    assert_eq!(
        verdict(200, r#"{"ok":false,"error":"channel_not_found"}"#),
        Verdict::Permanent,
        "a deleted channel refuses identically on every retry"
    );
}

/// A 200 that is not a Slack answer at all — a proxy's error page, a
/// captive portal. Not acceptance.
#[test]
fn test_a_200_that_is_not_slack_json_is_not_delivered() {
    for payload in ["", "not json", "[]", r#"{"ok":"true"}"#, "{}"] {
        assert_ne!(
            verdict(200, payload),
            Verdict::Delivered,
            "`{payload}` is not Slack saying it accepted the message"
        );
    }
}

#[test]
fn test_a_rate_limit_and_a_server_error_are_retryable() {
    for status in [429, 500, 502, 503] {
        assert_eq!(
            verdict(status, ""),
            Verdict::Retryable,
            "{status} is Slack asking for the message again later"
        );
    }
}

#[test]
fn test_other_client_errors_are_permanent() {
    for status in [400, 401, 403, 404] {
        assert_eq!(
            verdict(status, ""),
            Verdict::Permanent,
            "{status} will not change on a retry"
        );
    }
}

/// Every shape that names nowhere to post. Present-and-empty is in here
/// deliberately: it is the one a bare presence check would let through, and
/// it would be found at the vendor, a request later.
const UNPOSTABLE: [&str; 9] = [
    r#"{"channel_id":"","thread_ts":"1700000000.000100"}"#,
    r#"{"channel_id":"C123","thread_ts":""}"#,
    r#"{"channel_id":"C123"}"#,
    r#"{"thread_ts":"1700000000.000100"}"#,
    r#"{"channel_id":42,"thread_ts":"1700000000.000100"}"#,
    r#"{"channel_id":null,"thread_ts":"1700000000.000100"}"#,
    r#"{"channel_id":"C123","reply_thread_ts":"1700000000.000100"}"#,
    "{}",
    "not json",
];

/// Dimension 3.2 — an address naming nowhere is a permanent verdict, and it
/// is reached from the job alone: nothing has been read or requested.
#[test]
fn unreadable_address_is_permanent_without_a_request() {
    for stored in UNPOSTABLE {
        assert!(
            matches!(destination(&job(stored)), Err(Verdict::Permanent)),
            "`{stored}` must be refused before any request is built"
        );
    }
}

/// The answer is model output, so the body has to carry whatever a run
/// produced — through `serde`, never through interpolation.
#[test]
fn test_the_body_escapes_an_answer_carrying_json_punctuation() {
    let answer = "she said \"yes\"\nand {\"ok\": false}";
    let marker = AnswerMarker {
        fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
        event_id: "1700000000000-0".to_owned(),
    };
    let round_tripped: Option<serde_json::Value> = serde_json::to_vec(&Message {
        channel: "C123",
        thread_ts: "1700000000.000100",
        text: answer,
        metadata: marker.metadata(),
    })
    .ok()
    .and_then(|body| serde_json::from_slice(&body).ok());

    assert_eq!(
        round_tripped
            .as_ref()
            .and_then(|message| message.get("text"))
            .and_then(serde_json::Value::as_str),
        Some(answer),
        "what serde wrote, serde reads back whole"
    );
}

/// Every post carries the answer's marker as Slack message metadata, so a
/// repeat attempt can find this answer in the thread and post nothing.
#[test]
fn every_post_carries_its_answer_marker() {
    let marker = AnswerMarker {
        fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
        event_id: "1700000000000-0".to_owned(),
    };
    let body: Option<serde_json::Value> = serde_json::to_vec(&Message {
        channel: "C123",
        thread_ts: "1700000000.000100",
        text: "the answer",
        metadata: marker.metadata(),
    })
    .ok()
    .and_then(|body| serde_json::from_slice(&body).ok());
    let metadata = body.as_ref().and_then(|body| body.get("metadata"));

    assert_eq!(
        metadata
            .and_then(|metadata| metadata.get("event_type"))
            .and_then(serde_json::Value::as_str),
        Some(afd_connector::slack::ANSWER_EVENT_TYPE)
    );
    let payload = metadata.and_then(|metadata| metadata.get("event_payload"));
    for (field, expected) in [
        ("fleet_id", marker.fleet_id.as_str()),
        ("event_id", marker.event_id.as_str()),
    ] {
        assert_eq!(
            payload
                .and_then(|payload| payload.get(field))
                .and_then(serde_json::Value::as_str),
            Some(expected),
            "{field}"
        );
    }
}
