//! The seam types guffbolt-ai owns, read through guffbolt: each `guffbolt::`
//! path still names them, and a conversation saved while guffbolt owned them
//! reads back byte for byte.

use guffbolt::DriveState;
use guffbolt::agents::Status;
use serde_json::json;

use crate::script::Script;

/// The conversation the request is cached under.
const CONVERSATION: &str = "conversation-1";

/// A root's state with one child, saved while guffbolt still owned the seam
/// types: the child's transcript holds a provider's replay record and a tool
/// result carrying an image.
const SAVED: &str = r#"{"answers":["A red square."],"completed_steps":2,"children":{"next":1,"started":1,"children":[{"id":1,"parent":0,"depth":1,"calls":1,"tools":["read"],"status":{"Done":"A red square."},"transcript":[{"User":"Describe the logo."},{"Assistant":{"text":"","calls":[{"id":"call-1","name":"read","arguments":{"path":"logo.png"}}],"replay":{"content":[{"type":"thinking","thinking":"Open the file first.","signature":"sig-1"}]}}},{"ToolResult":{"call_id":"call-1","output":"logo.png, 4 bytes","image":{"kind":"Png","data":[137,80,78,71]}}},{"Assistant":{"text":"A red square.","calls":[],"replay":null}}],"state":{"answers":["A red square."],"completed_steps":2}}]}}"#;

#[test]
fn a_drive_state_saved_before_the_move_resumes() {
    let state: DriveState = serde_json::from_str(SAVED).expect("the saved state reads");

    let child = state.children().children.first().expect("one saved child");
    assert_eq!(child.status, Status::Done("A red square.".to_owned()));
    let replay = match child.transcript.get(1) {
        Some(guffbolt::Message::Assistant { replay, .. }) => Some(replay.record()),
        _ => None,
    };
    assert_eq!(
        replay.and_then(|record| record.pointer("/content/0/signature")),
        Some(&json!("sig-1")),
        "the provider's record reads back unopened"
    );
    let image = match child.transcript.get(2) {
        Some(guffbolt::Message::ToolResult { image, .. }) => image.as_ref(),
        _ => None,
    };
    assert_eq!(
        image.map(|image| (image.kind, image.data.as_ref())),
        Some((guffbolt::ImageKind::Png, [137, 80, 78, 71].as_slice())),
        "the read's result carries its image"
    );
    assert_eq!(
        serde_json::to_string(&state).expect("the state encodes"),
        SAVED,
        "it writes back byte for byte"
    );
}

/// Whether `model` takes images, asked through guffbolt-ai's own trait: a
/// model written against `guffbolt::Model` is the same trait's implementor.
fn accepts_images<M: guffbolt_ai::Model>(model: &M) -> bool {
    model.accepts_images()
}

#[test]
fn guffbolt_reexports_every_seam_type() {
    let parameters = json!({"type": "object"});
    let spec: guffbolt::ToolSpec<'_> = guffbolt_ai::ToolSpec {
        name: "read",
        description: "Reads a file.",
        parameters: &parameters,
    };
    let call: guffbolt::Call = guffbolt_ai::Call {
        id: "call-1".to_owned(),
        name: spec.name.to_owned(),
        arguments: json!({"path": "logo.png"}),
    };
    let replay = guffbolt::Replay::new(json!({"id": "msg_1"}));
    let image: guffbolt::Image = guffbolt_ai::Image {
        kind: guffbolt_ai::ImageKind::Png,
        data: b"PNG".as_slice().into(),
    };
    let kind: guffbolt::ImageKind = image.kind;
    let messages: [guffbolt::Message; 2] = [
        guffbolt_ai::Message::Assistant {
            text: String::new(),
            calls: vec![call.clone()],
            replay: replay.clone(),
        },
        guffbolt_ai::Message::ToolResult {
            call_id: call.id,
            output: "read".to_owned(),
            image: Some(image),
        },
    ];
    let request: guffbolt::Request<'_> = guffbolt_ai::Request {
        model: "model",
        instructions: "Answer.",
        messages: &messages,
        tools: &[spec],
        cache_key: CONVERSATION,
        max_output_tokens: None,
        single_attempt: false,
    };
    let mut usage: guffbolt::Usage = guffbolt_ai::Usage {
        input: 3,
        ..guffbolt_ai::Usage::default()
    };
    usage += guffbolt::Usage {
        output: 2,
        ..guffbolt::Usage::default()
    };
    let chunks: [guffbolt::Chunk; 2] = [
        guffbolt_ai::Chunk::Text {
            kind: guffbolt::TextKind::Answer,
            text: "done".to_owned(),
        },
        guffbolt_ai::Chunk::End(guffbolt::End { replay, cut: false }),
    ];

    assert_eq!(request.tools.first().map(|tool| tool.name), Some("read"));
    assert_eq!(request.messages.len(), 2);
    assert_eq!(kind.media_type(), "image/png");
    assert_eq!(usage.total(), 5);
    assert_eq!(
        chunks.last(),
        Some(&guffbolt_ai::Chunk::End(guffbolt_ai::End {
            replay: guffbolt_ai::Replay::new(json!({"id": "msg_1"})),
            cut: false,
        }))
    );
    assert!(
        !accepts_images(&Script::default()),
        "the scripted model, a `guffbolt::Model`"
    );
}
