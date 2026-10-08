//! A tool result's image as each wire carries it: after the result's text,
//! as base64 under its own media type.

#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use bytes::Bytes;
use rig_core::message::{
    DocumentSourceKind, ImageMediaType, Message as RigMessage, ToolResultContent, UserContent,
};

use super::tests::{CALL_ID, OUTPUT, built, conversation};
use crate::image_input::{ImageInput, ImageKind};
use crate::provider::Message;
use crate::registry::Wire;

/// An image a call read goes back inside its result, after the text, as
/// base64 under its media type: the shape rig turns into a Messages `image`
/// block and a Responses `input_image`.
#[test]
fn should_carry_a_tool_results_image_as_base64_content_after_its_text() {
    let mut messages = conversation();
    messages[2] = Message::ToolResult {
        call_id: CALL_ID.to_owned(),
        output: OUTPUT.to_owned(),
        image: Some(ImageInput {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG"),
        }),
    };

    let sent = built(Wire::Messages, &messages, &[], &[]);

    let RigMessage::User { content: answered } = &sent.chat_history[3] else {
        panic!("the result is a user turn: {:?}", sent.chat_history);
    };
    let UserContent::ToolResult(result) = &answered[0] else {
        panic!("the result leads the turn: {answered:?}");
    };
    let parts: Vec<&ToolResultContent> = result.content.iter().collect();
    let [
        ToolResultContent::Text(text),
        ToolResultContent::Image(image),
    ] = parts.as_slice()
    else {
        panic!("the text, then the image: {parts:?}");
    };
    assert_eq!(text.text, OUTPUT);
    // pin test: literal is the contract
    assert_eq!(
        image.data,
        DocumentSourceKind::Base64("iVBORw==".to_owned())
    );
    assert_eq!(image.media_type, Some(ImageMediaType::PNG));
    assert_eq!(image.detail, None, "each wire applies its own default");
}

/// Each kind the seam names goes out under its own media type.
#[test]
fn should_name_each_image_kinds_media_type() {
    for (kind, media_type) in [
        (ImageKind::Png, ImageMediaType::PNG),
        (ImageKind::Jpeg, ImageMediaType::JPEG),
        (ImageKind::Gif, ImageMediaType::GIF),
        (ImageKind::Webp, ImageMediaType::WEBP),
    ] {
        let mut messages = conversation();
        messages[2] = Message::ToolResult {
            call_id: CALL_ID.to_owned(),
            output: OUTPUT.to_owned(),
            image: Some(ImageInput {
                kind,
                data: Bytes::from_static(b"\x00"),
            }),
        };

        let sent = built(Wire::Responses, &messages, &[], &[]);

        let RigMessage::User { content: answered } = &sent.chat_history[3] else {
            panic!("the result is a user turn: {:?}", sent.chat_history);
        };
        let UserContent::ToolResult(result) = &answered[0] else {
            panic!("the result leads the turn: {answered:?}");
        };
        let Some(ToolResultContent::Image(image)) = result.content.get(1) else {
            panic!("{kind:?}: the image follows the text: {:?}", result.content);
        };
        assert_eq!(image.media_type, Some(media_type), "{kind:?}");
    }
}
