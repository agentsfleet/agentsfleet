//! A provider's record is saved exactly as the provider wrote it, and an
//! image kind is named by its media type.

use serde_json::json;

use super::{Call, ImageKind, Message, Replay};

#[test]
fn a_provider_record_is_saved_as_the_provider_wrote_it() {
    let record =
        json!({"content": [{"type": "reasoning", "text": "kept"}], "origin": {"id": "msg_1"}});
    let message = Message::Assistant {
        text: "Done.".to_owned(),
        calls: vec![Call {
            id: "call-1".to_owned(),
            name: "read".to_owned(),
            arguments: json!({"path": "README.md"}),
        }],
        replay: Replay::new(record.clone()),
    };
    let saved = serde_json::to_value(&message).unwrap_or_default();
    assert_eq!(saved.pointer("/Assistant/replay"), Some(&record));
    assert_eq!(
        serde_json::from_value::<Message>(saved).ok(),
        Some(message),
        "it reads back whole"
    );
}

#[test]
fn a_step_no_provider_recorded_holds_no_record() {
    assert!(Replay::default().is_empty());
    assert!(!Replay::new(json!({"content": []})).is_empty());
}

#[test]
fn an_image_kind_is_named_by_its_media_type() {
    let named = [
        (ImageKind::Png, "image/png"),
        (ImageKind::Jpeg, "image/jpeg"),
        (ImageKind::Gif, "image/gif"),
        (ImageKind::Webp, "image/webp"),
    ];

    for (kind, media_type) in named {
        assert_eq!(kind.media_type(), media_type);
    }
}
