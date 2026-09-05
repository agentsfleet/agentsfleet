use super::EventType;
use super::field;

#[test]
fn every_event_type_round_trips_between_storage_and_wire_spellings() {
    for (value, stored) in [
        (EventType::Chat, "chat"),
        (EventType::Webhook, "webhook"),
        (EventType::Cron, "cron"),
        (EventType::Continuation, "continuation"),
    ] {
        assert_eq!(value.as_str(), stored);
        assert_eq!(EventType::parse(stored), Some(value));
        let encoded = serde_json::to_value(value);
        assert_eq!(
            encoded.as_ref().ok().and_then(serde_json::Value::as_str),
            Some(stored)
        );
    }
    assert_eq!(EventType::parse("future_event"), None);
}

/// The wire spellings are the ones the shipped encoder wrote.
///
/// LITERALS on purpose, and this is the only place they appear as literals in
/// the unit tier. Every other test builds both the producer's entry and the
/// reader's expectation from the constants above, so a rename moves both sides
/// together and passes — while every entry already sitting on a stream, and
/// everything `event_envelope.zig` wrote, becomes undecodable. That is the
/// exact failure this crate was carved out to prevent, and without these five
/// lines nothing in the fast lane would catch it.
#[test]
fn the_wire_spellings_are_the_ones_the_encoder_shipped() {
    assert_eq!(field::ACTOR, "actor");
    assert_eq!(field::EVENT_TYPE, "type");
    assert_eq!(field::WORKSPACE_ID, "workspace_id");
    assert_eq!(field::REQUEST_JSON, "request");
    assert_eq!(field::CREATED_AT, "created_at");
}
