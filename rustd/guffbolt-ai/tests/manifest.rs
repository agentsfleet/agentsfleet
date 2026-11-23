//! What an embedder sees from outside the crate: a conversation built from the
//! root-level types saves and loads unchanged, so a stored transcript outlives
//! the process that wrote it.

use guffbolt_ai::{Call, Message, Replay};
use serde_json::json;

#[test]
fn a_conversation_built_from_the_root_types_saves_and_loads_unchanged() {
    let call = Call {
        id: "call-1".into(),
        name: "read".into(),
        arguments: json!({"path": "logo.png"}),
    };
    let conversation = vec![
        Message::User("Describe the logo.".into()),
        Message::Assistant {
            text: String::new(),
            calls: vec![call],
            replay: Replay::default(),
        },
    ];

    let saved = serde_json::to_string(&conversation).unwrap_or_default();
    let loaded: Vec<Message> = serde_json::from_str(&saved).unwrap_or_default();

    assert_eq!(loaded, conversation);
}
