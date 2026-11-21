//! An image a tool hands the model on the turn its result opens.
//!
//! The seam carries `guffbolt`'s image, its bytes and their kind; the wire
//! spells them. rig maps one tool-result image onto a Messages `image` block
//! and a Responses `input_image`, each from the same base64 content, so the
//! two wires that take an image take it from here
//! (`rig-core/src/providers/anthropic/completion.rs`,
//! `openai/responses_api/mod.rs`); Gemini and Ollama take it inside the
//! result or, where the wire refuses that, in the user message rig's history
//! adapter adds. Chat completions take text alone, which
//! [`InferenceApi::carries_images`](af_wire::inference::InferenceApi::carries_images) says.

use base64::prelude::{BASE64_STANDARD, Engine as _};
use guffbolt::{Image, ImageKind};
use rig_core::message::{DocumentSourceKind, ImageMediaType, ToolResultContent};

/// The media type rig names `kind` by, and the wires after it.
const fn media_type(kind: ImageKind) -> ImageMediaType {
    match kind {
        ImageKind::Png => ImageMediaType::PNG,
        ImageKind::Jpeg => ImageMediaType::JPEG,
        ImageKind::Gif => ImageMediaType::GIF,
        ImageKind::Webp => ImageMediaType::WEBP,
    }
}

/// `image` as rig carries it inside a tool result: base64, under its media
/// type, with no detail hint, so each wire applies its own default.
pub(crate) fn content(image: &Image) -> ToolResultContent {
    ToolResultContent::Image(rig_core::message::Image {
        data: DocumentSourceKind::Base64(BASE64_STANDARD.encode(&image.data)),
        media_type: Some(media_type(image.kind)),
        ..rig_core::message::Image::default()
    })
}
