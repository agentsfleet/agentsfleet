use super::EventType;

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
